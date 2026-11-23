//! Stopping every hub a stack's daemon started, as the stack goes.

#![cfg(test)]

use std::process::Child;

use super::{Stack, TIMEOUT};

impl Drop for Stack {
    /// Stops the hub, and every other the stack's daemon started, and waits for each to exit, so none outlives the test
    /// and none is running in its directory as that's removed. The app the test drove is still there, though its
    /// window has gone, and connects again as soon as its hub goes, which starts nothing once the daemon is closed. A
    /// hub that's gone already, such as one of another build that a test killed, is what was wanted.
    fn drop(&mut self) {
        let started = self.started.close();
        let stopped = client_core::stop_hub(&self.hub.paths.socket);
        let outlived: Vec<u32> = started.into_iter().filter_map(|mut hub| stop(&mut hub)).collect();
        // Not again in a test that failed, since a panic as it unwinds would end the run without saying why.
        if std::thread::panicking() {
            return;
        }
        assert!(stopped.is_ok(), "the hub didn't stop: {stopped:?}");
        assert!(
            outlived.is_empty(),
            "hubs the test started were still running after it, and were killed: {outlived:?}"
        );
    }
}

/// Stops `hub`, if it's still running, as a signal does, since it can have started as the last one stopped, before it
/// listened, and waits for it to exit. Returns its pid if it didn't exit in time, once it's been killed.
pub(super) fn stop(hub: &mut Child) -> Option<u32> {
    let running = |hub: &mut Child| matches!(hub.try_wait(), Ok(None));
    if running(hub) {
        process::signal_group(hub.id(), process::Signal::Terminate);
    }
    if waiting::thread::up_to(TIMEOUT, || !running(hub)) {
        return None;
    }
    let _ = hub.kill();
    let _ = hub.wait();
    Some(hub.id())
}
