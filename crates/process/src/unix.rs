//! Process groups and sessions, signals, and a process's start time, from `/proc` on Linux and from what the kernel
//! noted when it started on a Mac.

use std::os::unix::process::CommandExt as _;
use std::process::Command;

use crate::{RunningProgram, Signal};

fn number(signal: Signal) -> i32 {
    match signal {
        Signal::Hangup => libc::SIGHUP,
        Signal::Interrupt => libc::SIGINT,
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    }
}

pub(crate) fn own_group(command: &mut Command) {
    command.process_group(0);
}

/// Nothing, since a Unix program has no window unless it opens one.
pub(crate) fn without_window(_command: &mut Command) {}

/// In a session of its own.
pub(crate) fn detach(command: &mut Command) {
    // SAFETY: setsid is async-signal-safe, which is all pre_exec requires.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// It does nothing for PID 0 or 1, because `kill(-0)` would signal our own group, and -1 means every process we're
/// allowed to signal.
pub(crate) fn signal_group(leader: u32, signal: Signal) {
    let Some(group) = i32::try_from(leader).ok().filter(|pid| *pid > 1) else {
        return;
    };
    // SAFETY: kill(2) takes any PID and signal, and has no memory effects.
    unsafe { libc::kill(-group, number(signal)) };
}

pub(crate) fn group_running(leader: u32) -> bool {
    let Some(group) = i32::try_from(leader).ok().filter(|pid| *pid > 1) else {
        return false;
    };
    // SAFETY: kill(2) takes any PID, and signal 0 only asks whether it could be sent.
    let sent = unsafe { libc::kill(-group, 0) };
    // A group of another user's is running too.
    sent == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Nothing to do, since what a child is given takes the place of this process's own.
pub(crate) fn keep_stdio_from_children() {}

/// On Linux, the clock ticks from when the machine started, as `/proc` gives it, and which of its starts that was,
/// since the count begins again with each, as `TICKS@BOOT`. `ps` prints the start there as the time the machine
/// started, worked out again from the wall clock each time, plus that count, so it changes whenever the clock is set,
/// as WSL's is every 30 seconds or so when it catches up with Windows's.
#[cfg(target_os = "linux")]
pub(crate) fn start_time(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    Some(format!("{}@{}", ticks_from_boot(&stat)?, boot()))
}

/// On a Mac, the wall clock's seconds and microseconds when the process started, as the kernel noted them then, as
/// `SECONDS.MICROSECONDS`. `ps` prints the same to the second in local time, with no zone, so a change of daylight
/// saving time changes it, as does a `TZ` that differs between the process writing a record and the one reading it.
#[cfg(target_os = "macos")]
pub(crate) fn start_time(pid: u32) -> Option<String> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let size = libc::c_int::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: proc_bsdinfo is integers and arrays of them, for which all zeroes is a value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    // SAFETY: `info` outlives the call, and it writes no more than `size` bytes to it, which is its size. An `arg` of
    // 1 has it answer for a process that has exited and not been waited for, which `ps` still lists.
    let written = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 1, (&raw mut info).cast(), size) };
    (written == size).then(|| format!("{}.{:06}", info.pbi_start_tvsec, info.pbi_start_tvusec))
}

/// Whether `pid` is the process that started at `started`. A record an older build wrote has the start as `ps` printed
/// it, which has spaces where neither form above does, and a hub carrying on with that build's workers still has to
/// tell whether they're running.
pub(crate) fn started_at(pid: u32, started: &str) -> bool {
    if started.contains(char::is_whitespace) {
        return printed_start_time(pid).as_deref() == Some(started);
    }
    start_time(pid).as_deref() == Some(started)
}

/// Which of the machine's starts this is, or nothing if `/proc` doesn't say, which still tells apart the processes of
/// this one.
#[cfg(target_os = "linux")]
fn boot() -> &'static str {
    static BOOT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BOOT.get_or_init(|| {
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default().trim().to_string()
    })
}

fn printed_start_time(pid: u32) -> Option<String> {
    let output = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!started.is_empty()).then_some(started)
}

/// The 22nd field of `/proc/PID/stat`. The second is the program's name in brackets, which can hold spaces and
/// brackets of its own, so the fields are counted from the third, after the last `)`.
#[cfg(target_os = "linux")]
fn ticks_from_boot(stat: &str) -> Option<&str> {
    let (_, after_name) = stat.rsplit_once(')')?;
    after_name.split_ascii_whitespace().nth(22 - 3).filter(|field| is_count(field))
}

#[cfg(target_os = "linux")]
fn is_count(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// None, since nothing a process has open keeps a folder from being moved on Unix, which is all
/// [`running_in`](crate::running_in) is for.
pub(crate) fn running_programs() -> Vec<RunningProgram> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::Stdio;

    use super::*;

    #[test]
    fn a_signal_reaches_the_whole_group() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & echo $!; wait"]).stdout(Stdio::piped());
        own_group(&mut command);
        let mut leader = command.spawn().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(leader.stdout.take().unwrap()), &mut line).unwrap();
        let sleeper: u32 = line.trim().parse().unwrap();

        signal_group(leader.id(), Signal::Kill);

        assert_eq!(leader.wait().unwrap().signal(), Some(libc::SIGKILL));
        waiting::thread::until(
            std::time::Duration::from_secs(5),
            "the child of the group leader is still running",
            || start_time(sleeper).is_none(),
        );
    }

    /// Its leader can go first, as a shell can before the job it ran in front.
    #[test]
    fn a_group_runs_until_the_last_of_it_has_gone() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & exit"]);
        own_group(&mut command);
        let mut leader = command.spawn().unwrap();

        leader.wait().unwrap();
        let outlasted = group_running(leader.id());
        signal_group(leader.id(), Signal::Kill);

        assert!(outlasted, "the group went with its leader");
        // The one left is init's to reap, in its own time.
        waiting::thread::until(
            std::time::Duration::from_secs(5),
            "the group is still running after it was killed",
            || !group_running(leader.id()),
        );
        assert!(!group_running(0) && !group_running(1));
    }

    #[test]
    fn a_detached_process_leads_a_session_of_its_own() {
        let mut command = Command::new("sleep");
        command.arg("30");
        detach(&mut command);

        let mut started = command.spawn().unwrap();

        let pid = libc::pid_t::try_from(started.id()).unwrap();
        // SAFETY: getsid takes any PID, and 0 for this process's own, and touches no memory.
        let (its, ours) = unsafe { (libc::getsid(pid), libc::getsid(0)) };
        started.kill().unwrap();
        started.wait().unwrap();
        assert_eq!(its, pid, "it isn't the leader of a session");
        assert_ne!(its, ours, "it's in this process's session");
    }

    #[test]
    fn pids_that_would_signal_everything_are_refused() {
        // If these went through, this test's own process would be killed.
        signal_group(0, Signal::Kill);
        signal_group(1, Signal::Kill);
        signal_group(u32::MAX, Signal::Kill);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_start_is_counted_after_the_programs_name_whatever_it_holds() {
        let stat = "4242 (a ) (b 7) S 1 4242 4242 0 -1 4194560 120 0 0 0 1 2 0 0 20 0 1 0 987654 2490368 166 \
                    18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0\n";

        assert_eq!(ticks_from_boot(stat), Some("987654"));
        assert_eq!(ticks_from_boot("4242 (cut short) S 1"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_start_on_linux_is_a_count_of_ticks_from_this_boot() {
        let started = start_time(std::process::id()).unwrap();

        let (ticks, boot) = started.split_once('@').unwrap();
        assert!(is_count(ticks), "{started:?} isn't a count of ticks");
        let this_boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
        assert_eq!(boot, this_boot.trim());
        assert!(!boot.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_start_on_a_mac_is_the_wall_clock_when_it_started() {
        let before = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();

        let started = start_time(child.id());

        child.kill().unwrap();
        child.wait().unwrap();
        let (seconds, micros) = started.as_deref().unwrap().split_once('.').unwrap();
        let started = std::time::Duration::new(seconds.parse().unwrap(), 0)
            + std::time::Duration::from_micros(micros.parse().unwrap());
        assert!(started + std::time::Duration::from_secs(1) >= before, "{started:?} is before it started");
        assert!(started < before + std::time::Duration::from_secs(60), "{started:?} is long after it started");
    }

    /// As `ps` still lists it, which the local provider's test of stopping an orphan counts on.
    #[test]
    fn a_process_that_has_exited_has_its_start_until_its_waited_for() {
        let mut child = Command::new("true").spawn().unwrap();
        let started = start_time(child.id()).unwrap();
        let pid = libc::id_t::from(child.id());
        // SAFETY: siginfo_t is integers, pointers and unions of them, for which all zeroes is a value.
        let mut exited: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `exited` outlives the call. WNOWAIT leaves the child to be waited for again below.
        let waited = unsafe { libc::waitid(libc::P_PID, pid, &raw mut exited, libc::WEXITED | libc::WNOWAIT) };
        assert_eq!(waited, 0, "{}", std::io::Error::last_os_error());

        let still = start_time(child.id());

        child.wait().unwrap();
        assert_eq!(still, Some(started));
    }

    #[test]
    fn a_start_as_ps_printed_it_still_identifies_a_running_process() {
        let printed = printed_start_time(std::process::id()).unwrap();

        assert!(started_at(std::process::id(), &printed));
        assert!(!started_at(std::process::id(), "Thu Jan  1 00:00:00 1970"));
    }
}
