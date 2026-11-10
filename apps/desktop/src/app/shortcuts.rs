//! The shortcuts the window answers for repositories, workspaces and chats, which the File and View menus' items
//! share, and what those menus say about the selected workspace. macOS greys out an item when nothing where focus is takes its action, so each shortcut is
//! taken only where it can act.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, Div, InteractiveElement};

use super::AppView;
use crate::menus;
use crate::workspace_pane::{Showing, WorkspacePane};
use crate::{
    AddRepository, ArchiveWorkspace, CloseChat, CreatePullRequest, MarkWorkspace, NewChat, NewWorkspace, NextChat,
    NextWorkspace, PreviousChat, PreviousWorkspace, StopAgent, ToggleDiff,
};

impl AppView {
    /// Takes the shortcuts that can act now.
    pub(super) fn with_shortcuts(&self, root: Div, cx: &mut Context<Self>) -> Div {
        let connected = self.client().is_some();
        root.when(connected && self.current_repo().is_some(), |root| {
            root.on_action(cx.listener(|t, _: &NewWorkspace, window, cx| t.new_workspace_here(window, cx)))
        })
        .when(connected, |root| root.on_action(cx.listener(|t, _: &AddRepository, _, cx| t.add_repository(cx))))
        .when(!self.visible_workspaces().is_empty(), |root| {
            root.on_action(cx.listener(|t, _: &NextWorkspace, _, cx| t.step_selection(1, cx)))
                .on_action(cx.listener(|t, _: &PreviousWorkspace, _, cx| t.step_selection(-1, cx)))
        })
        // Home hides the selected workspace, so nothing acts on it unseen.
        .when(self.selected_pane().is_some() && !self.home_showing(), |root| with_workspace_shortcuts(root, cx))
    }

    /// Gives the menu bar again when what it says about the selected workspace changes, such as Show Diff becoming
    /// Hide Diff, since macOS only shows what it was last given. Cheap when nothing has changed, so every frame can
    /// ask, which catches every way it can change.
    pub(super) fn keep_menus_current(&mut self, cx: &mut Context<Self>) {
        let pane = self.selected_pane().map(|pane| pane.read(cx));
        let says = menus::Says {
            diff_open: pane.is_some_and(|pane| pane.changes_open(cx)),
            file_tab_showing: pane.is_some_and(|pane| pane.showing() == Showing::File),
        };
        if says != self.menus_say {
            self.menus_say = says;
            // Not while the window draws.
            cx.defer(move |cx| menus::show(says, cx));
        }
    }

    fn update_selected_pane(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut WorkspacePane, &mut Context<WorkspacePane>),
    ) {
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, update);
        }
    }
}

/// What acts on the selected workspace, which there has to be.
fn with_workspace_shortcuts(root: Div, cx: &mut Context<AppView>) -> Div {
    root.on_action(cx.listener(|t, _: &ToggleDiff, _, cx| t.update_selected_pane(cx, WorkspacePane::toggle_changes)))
        .on_action(cx.listener(|t, _: &CreatePullRequest, _, cx| {
            t.update_selected_pane(cx, WorkspacePane::create_pull_request);
        }))
        .on_action(cx.listener(|t, _: &NewChat, _, cx| t.update_selected_pane(cx, WorkspacePane::new_chat)))
        .on_action(cx.listener(|t, _: &CloseChat, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.close_tab(window, cx));
        }))
        .on_action(cx.listener(|t, _: &NextChat, _, cx| t.update_selected_pane(cx, |pane, cx| pane.step_chat(1, cx))))
        .on_action(
            cx.listener(|t, _: &PreviousChat, _, cx| t.update_selected_pane(cx, |pane, cx| pane.step_chat(-1, cx))),
        )
        .on_action(cx.listener(|t, _: &StopAgent, _, cx| t.update_selected_pane(cx, WorkspacePane::stop_agent)))
        // As the selected workspace's row offers them, for the row of the workspace showing.
        .on_action(cx.listener(|t, _: &ArchiveWorkspace, _, cx| {
            if let Some(id) = t.selected.clone() {
                t.archive(id, false, cx);
            }
        }))
        .on_action(cx.listener(|t, _: &MarkWorkspace, _, cx| {
            if let Some((mark, chats)) = t.selected.as_ref().and_then(|id| t.workspace_marking(id, cx)) {
                t.mark_chats(mark, chats, cx);
            }
        }))
}
