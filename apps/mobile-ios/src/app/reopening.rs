//! Opening the app where it was left, as `client_core::this_device` keeps it: on a workspace's chat, or on the list for
//! one left there. iOS can end an app in the background without a word, so someone coming back to it hasn't always
//! left the chat they had open.

use gpui_kit::{App, Context, Window};
use protocol::model::WorkspaceId;

use super::MobileApp;
use crate::hub_link::listed;

impl MobileApp {
    /// Opens the chat of the workspace the app was left on, if it's still listed. Once for each hub followed, as soon as
    /// the phone has its records, and not over anything opened before that, such as the hub's own screen.
    pub(super) fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(followed) = self.followed.as_mut().filter(|followed| followed.reopens) else { return };
        let link = followed.link.read(cx);
        let Some(records) = link.ready().map(|hub| hub.feed.snapshot()) else { return };
        followed.reopens = false;
        if self.open.chat.is_some() || self.open.panel.is_some() {
            return;
        }
        let can_show = |id: &WorkspaceId| records.workspace(id).is_some_and(listed);
        let last = self.preferences.last_workspace(Some(&link.paired().fingerprint)).cloned();
        match last.filter(can_show) {
            Some(workspace) => self.open_workspace(&workspace, window, cx),
            None => self.left_on(None, cx),
        }
    }

    /// Keeps `workspace` as the one whose chat is open, for the app to open on again, or with none, the list.
    pub(super) fn left_on(&mut self, workspace: Option<&WorkspaceId>, cx: &App) {
        if let Some(followed) = &self.followed {
            self.preferences.left_on(Some(&followed.link.read(cx).paired().fingerprint), workspace);
        }
    }
}
