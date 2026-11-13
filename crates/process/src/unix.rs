//! Process groups and signals, and a process's start time: from `/proc` on Linux, and from `ps` elsewhere.

use std::os::unix::process::CommandExt as _;
use std::process::Command;

use crate::Signal;

fn number(signal: Signal) -> i32 {
    match signal {
        Signal::Interrupt => libc::SIGINT,
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    }
}

pub(crate) fn own_group(command: &mut Command) {
    command.process_group(0);
}

pub(crate) fn own_session(command: &mut Command) {
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

/// Nothing to do, since what a child is given takes the place of this process's own.
pub(crate) fn keep_stdio_from_children() {}

/// As `ps` prints it.
#[cfg(not(target_os = "linux"))]
pub(crate) fn start_time(pid: u32) -> Option<String> {
    let output = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!started.is_empty()).then_some(started)
}

/// Which boot this is, so a record made before the machine restarted can't match a process that has the same PID and
/// start tick after it.
#[cfg(target_os = "linux")]
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// The clock tick since boot at which the kernel started the process, after the ID of the boot. Linux keeps no time of
/// day for a process, so `ps` works one out from this tick and the time the system booted, which the kernel gives in
/// whole seconds, worked out from the wall clock as it is now. Whenever the wall clock is stepped, as NTP or a virtual
/// machine's time sync does, that can move by a second or more while the process runs, and a record of it made from
/// `ps` would no longer match.
#[cfg(target_os = "linux")]
pub(crate) fn start_time(pid: u32) -> Option<String> {
    let tick = start_tick(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)?;
    let boot = std::fs::read_to_string(BOOT_ID).ok()?;
    Some(format!("{} {tick}", boot.trim()))
}

/// The 22nd field of a process's `stat`. The second is its name, in parentheses, which can hold anything, spaces and
/// parentheses included, so the fields are counted from after the last parenthesis.
#[cfg(target_os = "linux")]
fn start_tick(stat: &str) -> Option<u64> {
    let (_, after_name) = stat.rsplit_once(')')?;
    after_name.split_whitespace().nth(19)?.parse().ok()
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while start_time(sleeper).is_some() {
            assert!(std::time::Instant::now() < deadline, "the child of the group leader is still running");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
    fn the_start_tick_is_found_whatever_the_name_holds() {
        // A process named `a) (b c`, which started at tick 987654.
        let stat = "123 (a) (b c) S 1 123 123 0 -1 4194560 120 0 0 0 0 0 0 0 20 0 1 0 987654 2367488 200 0\n";

        assert_eq!(start_tick(stat), Some(987_654));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_from_another_boot_is_not_the_one_running() {
        let pid = std::process::id();
        let started = start_time(pid).unwrap();
        let (_, tick) = started.split_once(' ').unwrap();
        let this_boot = crate::ProcessRecord { pid, started: started.clone() };
        let before_a_restart =
            crate::ProcessRecord { pid, started: format!("00000000-0000-0000-0000-000000000000 {tick}") };

        assert!(this_boot.is_running());
        assert!(!before_a_restart.is_running());
    }
}
