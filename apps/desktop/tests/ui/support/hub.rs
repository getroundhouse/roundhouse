//! Losing the hub, and waiting for the next one.

#![cfg(test)]

use std::process::Command;

use client_core::Connection;
use desktop::app::Linking;
use gpui_kit::{AppContext as _, ElementId};
use local_hub::Daemon;

use super::{Stack, Ui, click};

impl Stack {
    /// Kills the hub the way a crash would, with no chance to tidy up. Returns the pid it had.
    pub(crate) fn crash_hub(&self) -> u32 {
        let pid = Connection::connect(&self.hub.paths.socket, "test").unwrap().hub_status().unwrap().pid;
        assert!(Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success());
        pid
    }

    /// Gives the app a daemon that isn't there, so it can't start a hub of its own, and waits while the test
    /// changes things. Returns the real one, for the test to start a hub with.
    pub(crate) fn without_a_daemon(&mut self) -> Daemon {
        std::mem::replace(&mut self.hub.daemon, Daemon::in_place("/nonexistent/roundhoused"))
    }
}

impl Ui<'_> {
    /// Clicks it in the window as last drawn, and leaves what that starts for the next frame to run, so a test can make
    /// several such clicks and stop the hub while what they ask for is still to be sent.
    pub(crate) fn click_and_leave(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.cx.update_window(self.handle, |_, window, cx| click(window, id, cx)).unwrap();
    }

    /// Stops the hub, and waits for the app to find it can't start another, as for a stack
    /// [`without_a_daemon`](Stack::without_a_daemon).
    pub(crate) fn lose_the_hub(&mut self, stack: &Stack) {
        client_core::stop_hub(&stack.hub.paths.socket).unwrap();
        self.wait("the app to find no hub", |ui| matches!(ui.view().linking(), Linking::Failed(_)));
    }

    /// Waits for the app to be connected again, to a hub other than the one with this pid. It asks with
    /// `Connection::connect`, never `stack.client()`, which would start that hub itself if it asked before the app had
    /// noticed the old one go. The test would then carry on while the app was still reconnecting.
    pub(crate) fn wait_for_new_hub(&mut self, stack: &Stack, pid: u32) {
        self.wait("a new hub", |ui| {
            matches!(ui.view().linking(), Linking::Ready)
                && Connection::connect(&stack.hub.paths.socket, "test")
                    .is_ok_and(|c| c.hub_status().is_ok_and(|s| s.pid != pid))
        });
    }
}
