//! The test a test's hub stops with.

use std::time::Duration;

use process::ProcessRecord;
#[cfg(any(test, feature = "testing"))]
use protocol::paths::ENV_TEST_PROCESS;

/// The process of the test a hub was started for, which the hub stops with once it has gone, as a signal stops it:
/// with its workers and their agents. A test's hub runs apart from the test, as any a client starts does, so nothing
/// else ends it when the test dies before stopping it, as one that's killed, interrupted or timed out does. A worker
/// that can outlive its hub is given the same, from its hub's environment, and stops with its agents once the test has
/// gone while it has no hub, since a hub killed before its test could stop it, or start the next, leaves it running.
/// Only a hub built with the `testing` feature has one, as only that one takes a test's [`Timers`](crate::Timers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestProcess(ProcessRecord);

/// How often a test's hub looks whether its test has gone. A look reads one process's start time, which costs a hub
/// little. Stopping takes longer, as long as its workers take to stop.
const LOOK_EVERY: Duration = Duration::from_millis(100);

impl TestProcess {
    /// None, whatever `var` says, since only a build with the `testing` feature has one, and the product is never built
    /// with it.
    #[cfg(not(any(test, feature = "testing")))]
    pub fn from_env(_var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        None
    }

    /// The process whose PID `var` gives as [`ENV_TEST_PROCESS`], as it is when this is read, which is as the hub or
    /// the worker starts, while the test that started it is running, so a PID used again later isn't taken for it. One that's gone
    /// by then is gone at once.
    ///
    /// # Panics
    ///
    /// On a value that isn't a PID, which only a mistake in a test gives it.
    #[cfg(any(test, feature = "testing"))]
    #[expect(clippy::expect_used, reason = "only a test sets it, to a PID, so one that isn't is a mistake in the test")]
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let pid = var(ENV_TEST_PROCESS)?.parse().expect("a test process's PID");
        Some(Self(ProcessRecord { pid, started: process::start_time(pid).unwrap_or_default() }))
    }

    /// Returns once `test` has gone, and never for a process with none, as the product's hub and workers are.
    pub async fn until_gone(test: Option<&Self>) {
        match test {
            Some(test) => test.gone().await,
            None => std::future::pending().await,
        }
    }

    /// Returns once the test process has gone.
    #[expect(clippy::disallowed_methods, reason = "the pace a test's hub looks whether its test has gone")]
    async fn gone(&self) {
        while self.0.is_running() {
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
    fn a_hub_takes_the_test_process_it_was_given_as_it_is_now() {
        let this = std::process::id();

        let test = given(this).unwrap();

        assert_eq!(test, TestProcess(ProcessRecord { pid: this, started: process::start_time(this).unwrap() }));
        assert_eq!(TestProcess::from_env(|_| None), None);
    }

    /// On tokio's paused clock, which moves on through each wait between looks.
    #[tokio::test(start_paused = true)]
    async fn a_test_process_that_has_gone_is_found_gone_and_one_running_isnt() {
        let gone = TestProcess(ProcessRecord { pid: std::process::id(), started: "long ago".to_string() });
        tokio::time::timeout(LOOK_EVERY, gone.gone()).await.expect("it has gone");

        let running = given(std::process::id()).unwrap();
        let looked = tokio::time::timeout(LOOK_EVERY * 10, running.gone()).await;
        assert!(looked.is_err(), "it's this process, which is running");
        let none = tokio::time::timeout(LOOK_EVERY * 10, TestProcess::until_gone(None)).await;
        assert!(none.is_err(), "there's none to go");
    }

    #[tokio::test(start_paused = true)]
    async fn a_test_process_gone_before_the_hub_read_it_is_gone_at_once() {
        let mut exited = std::process::Command::new("true").spawn().unwrap();
        let pid = exited.id();
        exited.wait().unwrap();

        let test = given(pid).unwrap();

        tokio::time::timeout(LOOK_EVERY, test.gone()).await.expect("it had gone");
    }
}
