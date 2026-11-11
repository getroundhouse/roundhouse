//! The new-workspace dialog's side of the app: opening it, telling it where a workspace can be made and whether one can
//! be made now, taking what it asks to make, and giving back a prompt for a workspace that couldn't be made.

use client_core::FileToSend;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, AppContext as _, Context, Entity, WeakEntity, Window};
use protocol::model::{PermissionMode, RepoId, WorkspaceId};

use crate::app::AppView;
use crate::new_workspace::{self, DialogEvent, NewWorkspaceDialog, Offer};

impl AppView {
    /// Opens the new-workspace dialog for `repo`, with whatever was left in it last time. Only one dialog shows at a
    /// time, so this does nothing while another is open. It opens while a workspace is being made, and its Create waits.
    pub fn open_new_workspace(&mut self, repo: RepoId, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let dialog = self.dialog(repo.clone(), window, cx);
        let offer = self.offer();
        dialog.update(cx, |dialog, cx| {
            dialog.set_repo(repo, cx);
            dialog.set_offer(offer, cx);
        });
        new_workspace::open(&dialog, closed(cx.weak_entity()), window, cx);
    }

    /// The dialog, made the first time it's asked for.
    fn dialog(&mut self, repo: RepoId, window: &mut Window, cx: &mut Context<Self>) -> Entity<NewWorkspaceDialog> {
        if let Some(dialog) = &self.new_workspace {
            return dialog.clone();
        }
        let offer = self.offer();
        let dialog = cx.new(|cx| NewWorkspaceDialog::new(repo, offer, window, cx));
        cx.subscribe_in(&dialog, window, |t, dialog, event: &DialogEvent, window, cx| match event {
            DialogEvent::Create => t.create_from(dialog, window, cx),
            DialogEvent::Error(message) => t.show_error(message.clone(), cx),
        })
        .detach();
        self.new_workspace = Some(dialog.clone());
        dialog
    }

    /// Makes what the dialog asks for, and closes it. If it can't be made now, what the dialog held stays in it.
    fn create_from(&mut self, dialog: &Entity<NewWorkspaceDialog>, window: &mut Window, cx: &mut Context<Self>) {
        let request = dialog.update(cx, |dialog, cx| dialog.take_request(window, cx));
        match self.create_workspace(request, cx) {
            Ok(()) => {
                window.close_dialog(cx);
                self.dialog_closed(window, cx);
            }
            Err(request) => dialog.update(cx, |dialog, cx| {
                dialog.give_back(request.prompt, request.files, request.permission_mode, window, cx);
            }),
        }
    }

    /// Where a workspace can be made, in the sidebar's order, and whether one can be made now.
    fn offer(&self) -> Offer {
        let repos = self.repos.iter().map(|repo| (repo.id.clone(), repo.name.clone())).collect();
        Offer { repos, can_create: self.can_create() }
    }

    /// Tells the dialog what's changed since it was last told, each time the window draws.
    pub(in crate::app) fn keep_dialog_current(&self, cx: &mut Context<Self>) {
        if let Some(dialog) = &self.new_workspace {
            let offer = self.offer();
            dialog.update(cx, |dialog, cx| dialog.set_offer(offer, cx));
        }
    }

    /// Puts back a prompt for a workspace that couldn't be made, with the mode it was to start in, and opens the dialog
    /// to show where it went, unless another dialog is open.
    pub(super) fn give_back_to_dialog(
        &mut self,
        text: String,
        files: Vec<FileToSend>,
        mode: PermissionMode,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.new_workspace.clone() else { return };
        let this = cx.weak_entity();
        let _ = self.window.update(cx, |_, window, cx| {
            dialog.update(cx, |dialog, cx| dialog.give_back(text, files, mode, window, cx));
            if !window.has_active_dialog(cx) {
                new_workspace::open(&dialog, closed(this), window, cx);
            }
        });
    }

    /// Focuses the workspace's pane, unless a dialog is open, where typing meant for the dialog would go to the pane
    /// unseen. Then it's focused once the dialog closes. While Home shows, as when the selection goes away under it,
    /// its search keeps the keys.
    pub(in crate::app) fn focus_pane(&mut self, id: WorkspaceId, window: &mut Window, cx: &mut App) {
        if window.has_active_dialog(cx) {
            self.focus_after_dialog = Some(id);
        } else if let Some(pane) = self.panes.get(&id).filter(|_| !self.home_showing()) {
            pane.update(cx, |pane, cx| pane.focus_showing(window, cx));
        }
    }

    /// Closing the dialog gives focus back to what had it before, which is no longer drawn if another workspace was
    /// selected meanwhile, so that one is focused instead.
    fn dialog_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.focus_after_dialog.take().filter(|id| self.selected.as_ref() == Some(id)) {
            self.focus_pane(id, window, cx);
        }
    }
}

/// What the app does once the dialog has closed, however it closed. After the dialog has given focus back.
fn closed(this: WeakEntity<AppView>) -> impl Fn(&mut Window, &mut App) + 'static {
    move |window, cx| {
        let this = this.clone();
        window.defer(cx, move |window, cx| {
            let _ = this.update(cx, |t, cx| t.dialog_closed(window, cx));
        });
    }
}
