//! A real hub started before the app opens, which the app then uses rather than starting its own, with what a test
//! adds to its environment, such as a stand-in for `gh` at the front of its `PATH`, or a stand-in for GitHub as
//! `ENV_TEST_GITHUB_API`. It's stopped with the test, even a failed one.

#![cfg(test)]

use std::path::PathBuf;
use std::process::{Child, Stdio};

use client_core::Client;
use process::testing::WholeEnvironment;

use super::{Stack, TIMEOUT, binaries};

/// The hub, which is stopped as this goes.
pub(crate) struct HubAhead {
    hub: Child,
    socket: PathBuf,
}

impl Stack {
    /// Starts the hub in `environment`, which `hub_environment` gives with what a test adds, and waits for it to
    /// listen. Workers get the hub's own `PATH`, as `environment` sets it, since it has no login shell to ask.
    pub(crate) fn start_hub_ahead(&self, environment: &WholeEnvironment) -> HubAhead {
        let hub = environment
            .command(binaries().join("daemon_for_tests"))
            .args(["hub", "--on-demand"])
            .env(protocol::paths::ENV_HOME, &self.hub.paths.data_dir)
            .env(protocol::paths::ENV_WORKSPACES, &self.hub.paths.workspaces_root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let socket = self.hub.paths.socket.clone();
        waiting::thread::until(TIMEOUT, "timed out waiting for the hub to listen", || {
            Client::connect(&socket, "test").is_ok()
        });
        HubAhead { hub, socket }
    }
}

impl Drop for HubAhead {
    /// The way the app stops a hub, so it has removed its socket by the time the stack looks, and then as the stack
    /// stops a hub it started. Before the stack goes, so a stand-in the hub calls, which goes with this, is still there
    /// as it stops.
    fn drop(&mut self) {
        let _ = client_core::stop_hub(&self.socket);
        super::stopping::stop(&mut self.hub);
    }
}
