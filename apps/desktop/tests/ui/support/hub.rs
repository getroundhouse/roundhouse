//! Losing the hub, and waiting for the next one.

#![cfg(test)]

use std::path::PathBuf;
use std::process::Command;

use client_core::Client;
use desktop::app::Connection;

use super::{Stack, Ui};

impl Stack {
    /// Kills the hub the way a crash would, with no chance to tidy up. Returns the pid it had.
    pub(crate) fn crash_hub(&self) -> u32 {
        let pid = Client::connect(&self.hub.paths.socket, "test").unwrap().hub_status().unwrap().pid;
        assert!(Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success());
        pid
    }

    /// Gives the app a daemon that isn't there, so it can't start a hub of its own, and waits while the test
    /// changes things. Returns the real one, for the test to start a hub with.
    pub(crate) fn without_a_daemon(&mut self) -> PathBuf {
        std::mem::replace(&mut self.hub.daemon, "/nonexistent/roundhoused".into())
    }
}

impl Ui<'_> {
    /// Waits for the app to be connected again, to a hub other than the one with this pid. It asks with
    /// `Client::connect`, never `stack.client()`, which would start that hub itself if it asked before the app had
    /// noticed the old one go. The test would then carry on while the app was still reconnecting.
    pub(crate) fn wait_for_new_hub(&mut self, stack: &Stack, pid: u32) {
        self.wait("a new hub", |ui| {
            matches!(ui.view().connection, Connection::Ready(_))
                && Client::connect(&stack.hub.paths.socket, "test")
                    .is_ok_and(|c| c.hub_status().is_ok_and(|s| s.pid != pid))
        });
    }
}
