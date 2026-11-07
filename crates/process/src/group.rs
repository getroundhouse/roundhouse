/// A signal sent to a process group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Interrupt,
    Terminate,
    Kill,
}

impl Signal {
    fn number(self) -> i32 {
        match self {
            Signal::Interrupt => libc::SIGINT,
            Signal::Terminate => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        }
    }
}

/// Signals the group led by `leader`, which has to have been started as a group leader, so a stop
/// reaches everything it started. It does nothing for PID 0 or 1, because `kill(-0)` would signal our
/// own group, and -1 means every process we're allowed to signal.
pub fn signal_group(leader: u32, signal: Signal) {
    let Some(group) = i32::try_from(leader).ok().filter(|pid| *pid > 1) else {
        return;
    };
    // SAFETY: kill(2) takes any PID and signal, and has no memory effects.
    unsafe { libc::kill(-group, signal.number()) };
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
    use std::process::{Command, Stdio};

    use super::*;

    #[test]
    fn a_signal_reaches_the_whole_group() {
        let mut leader = Command::new("sh")
            .args(["-c", "sleep 30 & echo $!; wait"])
            .process_group(0)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(leader.stdout.take().unwrap()), &mut line).unwrap();
        let sleeper: u32 = line.trim().parse().unwrap();

        signal_group(leader.id(), Signal::Kill);

        assert_eq!(leader.wait().unwrap().signal(), Some(libc::SIGKILL));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while crate::start_time(sleeper).is_some() {
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
