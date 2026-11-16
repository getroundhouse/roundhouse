//! A real hub started before the app opens, which the app then uses rather than starting its own, with what a test
//! adds to its environment, such as a stand-in for `gh` at the front of its `PATH`, or a stand-in for GitHub as
//! `ENV_TEST_GITHUB_API`. It's stopped with the test, even a failed one.

#![cfg(test)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use client_core::Client;

use super::{Stack, TIMEOUT, binaries};

/// The hub, which is stopped as this goes.
pub(crate) struct HubAhead {
    hub: Child,
    socket: PathBuf,
}

impl Stack {
    /// Starts the hub with `env` added to its environment, and waits for it to listen.
    pub(crate) fn start_hub_ahead(&self, env: &[(&str, String)]) -> HubAhead {
        let hub = Command::new(binaries().join("roundhoused"))
            .args(["hub", "--on-demand"])
            .env(protocol::paths::ENV_HOME, &self.hub.paths.data_dir)
            .env(protocol::paths::ENV_WORKSPACES, &self.hub.paths.workspaces_root)
            // No login shell, so workers get the hub's own PATH, as `env` may set it.
            .env("SHELL", "/usr/bin/false")
            .envs(env.iter().map(|(name, value)| (*name, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let socket = self.hub.paths.socket.clone();
        let deadline = Instant::now() + TIMEOUT;
        while Client::connect(&socket, "test").is_err() {
            assert!(Instant::now() < deadline, "timed out waiting for the hub to listen");
            std::thread::sleep(Duration::from_millis(10));
        }
        HubAhead { hub, socket }
    }
}

impl Drop for HubAhead {
    /// The way the app stops a hub, so it has removed its socket by the time the stack looks.
    fn drop(&mut self) {
        if let Ok(client) = Client::connect(&self.socket, "test") {
            let _ = client.hub_shutdown();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while matches!(self.hub.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.hub.kill();
        let _ = self.hub.wait();
    }
}
