//! A hub of another build, as `examples/stand_in_hub` describes, which says what a test has it say.

#![cfg(test)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::{Stack, TIMEOUT, binaries};

/// The hub, and its data directory, where a test writes what it says. Goes away with the test, even a failed one,
/// since nothing else would stop it.
pub(crate) struct StandInHub(pub(crate) Child, PathBuf);

impl StandInHub {
    /// Has it say this many agents are running from now on, as if some had started or finished.
    pub(crate) fn set_running(&self, agents: u32) {
        self.write("stand-in-running", agents);
    }

    fn write(&self, file: &str, number: u32) {
        // Whole or not at all, since it may read it at any moment.
        let next = self.1.join(format!("{file}.next"));
        std::fs::write(&next, number.to_string()).unwrap();
        std::fs::rename(&next, self.1.join(file)).unwrap();
    }
}

impl Drop for StandInHub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Stack {
    /// A hub of another build, run with these arguments.
    pub(crate) fn start_stand_in_hub(&self, args: &[&str]) -> StandInHub {
        let hub = Command::new(binaries().join("examples/stand_in_hub"))
            .args(args)
            .env(protocol::paths::ENV_HOME, &self.hub.paths.data_dir)
            .env(protocol::paths::ENV_WORKSPACES, &self.hub.paths.workspaces_root)
            // Keeping the test's output open would hang whatever reads it, if it were ever left running.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let hub = StandInHub(hub, self.hub.paths.data_dir.clone());
        let deadline = Instant::now() + TIMEOUT;
        while client_core::HubAdmin::connect(&self.hub.paths.socket).is_err() {
            assert!(Instant::now() < deadline, "timed out waiting for it to listen");
            std::thread::sleep(Duration::from_millis(10));
        }
        hub
    }
}
