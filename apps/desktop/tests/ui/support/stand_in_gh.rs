//! A real hub with a stand-in for `gh` at the front of its `PATH`, which reports the pull request a test has it
//! report, as the hub's own tests do in `apps/daemon/tests/e2e/workspaces.rs`.

#![cfg(test)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use client_core::Client;

use super::{Stack, TIMEOUT, Ui, binaries};

/// What `gh pr view` prints for a pull request in this state, such as `OPEN` or `MERGED`, for [`StandInGh::report`].
pub(crate) fn pull_request(state: &str) -> String {
    format!(r#"{{"number":7,"url":"https://example.com/pull/7","title":"T","state":"{state}","isDraft":false}}"#)
}

/// The hub, and the file its `gh` prints. Stops the hub with the test, even a failed one.
pub(crate) struct StandInGh {
    hub: Child,
    socket: PathBuf,
    answer: PathBuf,
}

impl StandInGh {
    /// Has `gh pr view` print this from now on, as GitHub would. Until a test calls this, it fails, as it does for a
    /// branch with no pull request.
    pub(crate) fn report(&self, json: &str) {
        // Whole or not at all, since the worker may run it at any moment.
        let next = self.answer.with_extension("next");
        std::fs::write(&next, json).unwrap();
        std::fs::rename(&next, &self.answer).unwrap();
    }
}

impl Drop for StandInGh {
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

impl Stack {
    /// Starts the hub before the app opens, which then uses it rather than starting its own.
    pub(crate) fn start_hub_with_stand_in_gh(&self) -> StandInGh {
        let bin = self.dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let answer = self.dir.path().join("gh-pr-view.json");
        // Anything but `gh pr view` fails, so the merge queue says nothing.
        let script = format!("#!/bin/sh\n[ \"$1 $2\" = 'pr view' ] && exec cat '{}'\nexit 1\n", answer.display());
        let gh = bin.join("gh");
        std::fs::write(&gh, script).unwrap();
        std::fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let hub = Command::new(binaries().join("roundhoused"))
            .args(["hub", "--on-demand"])
            .env(protocol::paths::ENV_HOME, &self.hub.paths.data_dir)
            .env(protocol::paths::ENV_WORKSPACES, &self.hub.paths.workspaces_root)
            // No login shell, so workers get this PATH, with the stand-in at its front.
            .env("PATH", path)
            .env("SHELL", "/usr/bin/false")
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
        StandInGh { hub, socket, answer }
    }
}

impl Ui<'_> {
    /// Lets the time pass after which the app reads the pull request showing again, and lets it read it.
    pub(crate) fn wait_out_a_pull_request_look(&mut self) {
        self.cx.executor().advance_clock(desktop::pull_request::READ_AGAIN);
        self.frame();
    }
}
