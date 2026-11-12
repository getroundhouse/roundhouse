//! Process groups and signals, and `ps` for a process's start time.

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
pub(crate) fn start_time(pid: u32) -> Option<String> {
    let output = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!started.is_empty()).then_some(started)
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
}
