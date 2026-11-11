//! What the app tells the pane it's waiting on the hub for, so the pane's own buttons for it wait too.

use gpui_kit::Context;

use super::WorkspacePane;
use crate::busy::Busy;

impl WorkspacePane {
    /// Whether the app is making, archiving or restoring a workspace, which the pull request's Archive waits for.
    pub(crate) fn set_busy(&mut self, busy: Busy, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }

    /// Whether the app is waiting for a new title for this workspace, which the header's button for one shows.
    pub(crate) fn set_retitling(&mut self, retitling: bool, cx: &mut Context<Self>) {
        self.retitling = retitling;
        cx.notify();
    }
}
