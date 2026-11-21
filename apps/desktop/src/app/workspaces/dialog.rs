//! The new-workspace dialog's side of the app. It opens the dialog, and tells it where a workspace can be made, whether
//! one can be made now, and what the hub keeps of it. It makes what the dialog asks for, and gives back a prompt for a
//! workspace that couldn't be made.

use client_core::Client;
use gpui_kit::{App, AppContext as _, Context, Entity, WeakEntity, Window};
use protocol::capability::approvals::ApprovalMode;
use protocol::model::{RepoId, WorkspaceId};

use super::made::GivenBack;
use crate::app::AppView;
use crate::new_workspace::{self, DialogEvent, Keeping, NewWorkspaceDialog, Offer};

impl AppView {
    /// Opens the new-workspace dialog for `repo`, with whatever was left in it last time. Only one dialog shows at a
    /// time, so this does nothing while another is open. It opens while a workspace is being made, and its Create waits.
    pub fn open_new_workspace(&mut self, repo: RepoId, window: &mut Window, cx: &mut Context<Self>) {
        if look::dialog_open(window, cx) {
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

    /// The dialog, made the first time it's asked for, with what the hub keeps of it.
    fn dialog(&mut self, repo: RepoId, window: &mut Window, cx: &mut Context<Self>) -> Entity<NewWorkspaceDialog> {
        if let Some(dialog) = &self.new_workspace {
            return dialog.clone();
        }
        // The connection there is, even one still catching up, which a dialog made now would otherwise hear of only on
        // the next, so it couldn't read back its files until then.
        let client = self.link.state().connected().cloned();
        let (offer, keeping) = (self.offer(), Keeping::new(self.drafts.clone(), client));
        let dialog = cx.new(|cx| NewWorkspaceDialog::new(repo, offer, keeping, window, cx));
        if let Some(kept) = self.feed().and_then(|feed| feed.snapshot().new_workspace_draft().cloned()) {
            dialog.update(cx, |dialog, cx| dialog.take_kept(kept, window, cx));
        }
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
                dialog.update(cx, NewWorkspaceDialog::closed);
                look::close_dialog(window, cx);
                self.dialog_closed(window, cx);
            }
            Err(request) => dialog.update(cx, |dialog, cx| {
                let new_workspace::Request { prompt, coming_back, mode, .. } = *request;
                dialog.give_back(prompt, coming_back, mode, window, cx);
            }),
        }
    }

    /// Where a workspace can be made, in the sidebar's order, whether one can be made now, and what the hub says of its
    /// agents.
    fn offer(&self) -> Offer {
        let repos = self.records.repos().iter().map(|repo| (repo.id.clone(), repo.name.clone())).collect();
        let model_lists = self.records.model_lists().cloned().collect();
        Offer { repos, can_create: self.can_create(), agents: self.records.agents().clone(), model_lists }
    }

    /// Tells the dialog what the hub keeps of it, as the records change, and as the keeper catches up, which can let
    /// it show a change made elsewhere that it held back. Once the window can be updated, since this can come while
    /// it's being drawn.
    pub(in crate::app) fn show_kept_in_dialog(&self, cx: &mut Context<Self>) {
        let (Some(dialog), Some(feed), handle) = (self.new_workspace.clone(), self.feed(), self.window) else {
            return;
        };
        let offer = self.offer();
        cx.defer(move |cx| {
            let Some(kept) = feed.snapshot().new_workspace_draft().cloned() else { return };
            let _ = handle.update(cx, |_, window, cx| {
                dialog.update(cx, |dialog, cx| {
                    dialog.set_offer(offer, cx);
                    // Such as the hub letting go of the list it follows, as when the default login changes.
                    dialog.follow_models(false, cx);
                    dialog.take_kept(kept, window, cx);
                });
            });
        });
    }

    /// Has the dialog read back on a new connection what the last couldn't.
    pub(in crate::app) fn reconnect_dialog(&self, client: &Client, cx: &mut Context<Self>) {
        if let Some(dialog) = &self.new_workspace {
            dialog.update(cx, |dialog, cx| dialog.reconnect(client, cx));
        }
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
        given_back: GivenBack,
        mode: Option<ApprovalMode>,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.new_workspace.clone() else { return };
        let GivenBack { prompt, coming_back, .. } = given_back;
        let _ = self.window.update(cx, |_, window, cx| {
            dialog.update(cx, |dialog, cx| dialog.give_back(prompt, coming_back, mode, window, cx));
        });
        self.present(cx, move |_, window, cx| {
            if !look::dialog_open(window, cx) {
                new_workspace::open(&dialog, closed(cx.weak_entity()), window, cx);
            }
        });
    }

    /// Focuses the workspace's pane, unless a dialog is open, where typing meant for the dialog would go to the pane
    /// unseen, or Settings is, which leaves the pane undrawn, so the keys would reach nothing, not even Settings'. Then
    /// it's focused once that closes. While Home shows, as when the selection goes away under it, its search keeps the
    /// keys, and nothing waits to take them once a dialog or Settings closes over it.
    pub(in crate::app) fn focus_pane(&mut self, id: WorkspaceId, window: &mut Window, cx: &mut App) {
        if !self.visible || self.home_showing() {
            return;
        }
        if look::dialog_open(window, cx) || self.settings_open() {
            self.focus_once_closed = Some(id);
        } else if let Some(pane) = self.panes.get(&id) {
            pane.update(cx, |pane, cx| pane.focus_showing(window, cx));
        }
    }

    /// Closing the dialog gives focus back to what had it before, which is no longer drawn if another workspace was
    /// selected meanwhile, so that one is focused instead.
    fn dialog_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.focus_once_closed.take().filter(|id| self.selected() == Some(id)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::hubs::tests::{host, pick, remote};

    #[gpui_kit::test]
    fn a_creation_failure_keeps_its_prompt_without_opening_over_another_hub(cx: &mut gpui_kit::TestAppContext) {
        let (_dir, window, host) = host(cx);
        let local = cx.read(|cx| host.read(cx).active());
        let dialog = cx.update(|cx| {
            window
                .update(cx, |_, window, cx| local.update(cx, |view, cx| view.dialog("repo".into(), window, cx)))
                .unwrap()
        });
        let other = pick(Some(remote("other")), window, &host, cx);
        local.update(cx, |view, cx| {
            view.give_back_to_dialog(
                GivenBack {
                    prompt: protocol::prompt::Prompt::new("Returned work"),
                    coming_back: Vec::new(),
                    in_the_dialog: false,
                },
                Some(protocol::fixtures::plan_mode()),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|cx| {
            window
                .update(cx, |_, window, cx| {
                    assert!(!look::dialog_open(window, cx));
                    assert!(other.read(cx).focus.is_focused(window));
                    assert_eq!(dialog.read(cx).prompt_text(cx), "Returned work");
                    assert!(!dialog.read(cx).is_shown());
                })
                .unwrap();
        });
        pick(None, window, &host, cx);
        cx.read(|cx| assert!(dialog.read(cx).is_shown()));
    }
}
