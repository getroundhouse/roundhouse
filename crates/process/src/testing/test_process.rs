//! The test a test's hub and workers stop with.

use std::time::Duration;

use protocol::testing::ENV_TEST_PROCESS;

use crate::ProcessRecord;

/// The process of the test a hub or a worker was started for, which the daemon for tests has it stop with once that has
/// gone: a hub as a signal stops it, with its workers and their agents, and a worker that can outlive its hub, with its
/// agents, while it has no hub. Each runs apart from the test, as any a client starts does, so nothing else ends it when
/// the test dies before stopping it, as one that's killed, interrupted or timed out does.
///
/// It's in the daemon for tests because neither the product's own ways to stop nor another process of the tests' does as
/// well. A signal needs something left to send it, a worker waits ten minutes for a next hub, and a hub has no such
/// limit, since it outlives its clients on purpose. A process that watches the test and stops what it left was tried, and
/// worked on a Mac and on Linux, but took several times this code, with its own way on each system to find what's
/// running, and a file left in place of each directory to keep a hub that's slow to start from making it again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestProcess(ProcessRecord);

/// How often a test's hub or worker looks whether its test has gone. A look reads one process's start time, which costs
/// little. Stopping takes longer, as long as its workers or agents take to stop.
const LOOK_EVERY: Duration = Duration::from_millis(100);

impl TestProcess {
    /// The process whose PID `var` gives as [`ENV_TEST_PROCESS`], as it is when this is read, which is as the hub or
    /// the worker starts, while the test that started it is running, so a PID used again later isn't taken for it. One
    /// that's gone by then is gone at once.
    ///
    /// # Panics
    ///
    /// On a value that isn't a PID, which only a mistake in a test gives it.
    #[expect(clippy::expect_used, reason = "only a test sets it, to a PID, so one that isn't is a mistake in the test")]
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let pid = var(ENV_TEST_PROCESS)?.parse().expect("a test process's PID");
        Some(Self(ProcessRecord { pid, started: crate::start_time(pid).unwrap_or_default() }))
    }

    /// Returns once `test` has gone, and never with none, as a test's hub or worker can be started with.
    #[expect(clippy::disallowed_methods, reason = "the pace a test's hub or worker looks whether its test has gone")]
    pub async fn until_gone(test: Option<Self>) {
        let Some(test) = test else { return std::future::pending().await };
        while test.0.is_running() {
            tokio::time::sleep(LOOK_EVERY).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(pid: u32) -> Option<TestProcess> {
        TestProcess::from_env(|asked| (asked == ENV_TEST_PROCESS).then(|| pid.to_string()))
    }

    #[test]
    fn the_test_process_given_is_taken_as_it_is_now() {
        let this = std::process::id();

        let test = given(this).unwrap();

        assert_eq!(test, TestProcess(ProcessRecord { pid: this, started: crate::start_time(this).unwrap() }));
        assert_eq!(TestProcess::from_env(|_| None), None);
    }

    /// On tokio's paused clock, which moves on through each wait between looks.
    #[tokio::test(start_paused = true)]
    async fn a_test_process_that_has_gone_is_found_gone_and_one_running_isnt() {
        let gone = TestProcess(ProcessRecord { pid: std::process::id(), started: "long ago".to_string() });
        tokio::time::timeout(LOOK_EVERY, TestProcess::until_gone(Some(gone))).await.expect("it has gone");

        let running = given(std::process::id()).unwrap();
        let looked = tokio::time::timeout(LOOK_EVERY * 10, TestProcess::until_gone(Some(running))).await;
        assert!(looked.is_err(), "it's this process, which is running");
        let none = tokio::time::timeout(LOOK_EVERY * 10, TestProcess::until_gone(None)).await;
        assert!(none.is_err(), "there's none to go");
    }

    /// On Unix, where a process that's been waited for has gone, and has no `true` to run on Windows.
    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn a_test_process_gone_before_it_was_read_is_gone_at_once() {
        let mut exited = std::process::Command::new("true").spawn().unwrap();
        let pid = exited.id();
        exited.wait().unwrap();

        let test = given(pid).unwrap();

        tokio::time::timeout(LOOK_EVERY, TestProcess::until_gone(Some(test))).await.expect("it had gone");
    }
}
