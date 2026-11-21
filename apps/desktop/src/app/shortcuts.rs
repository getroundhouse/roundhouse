//! The shortcuts the window answers for its sidebars, repositories, workspaces and chats, which the File and View
//! menus' items share, and what those menus say about the sidebars and the selected workspace. macOS greys out an item
//! when nothing where focus is takes its action, so each shortcut is taken only where it can act.

use client_core::marking::Mark;
use gpui_kit::component::input::Copy;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, Div, InteractiveElement};

use super::selection::Way;
use super::{AppView, corner};
use crate::menus;
use crate::sides::{Side, Sidebar};
use crate::workspace_pane::{Showing, WorkspacePane};
use crate::{
    AddRepository, ArchiveWorkspace, CloseChat, ConversationMode, CreatePullRequest, FindInChat, FindNext,
    FindPrevious, GoBack, GoForward, MarkWorkspace, NewChat, NewTerminal, NewWorkspace, NextChat, NextWorkspace,
    PreviousChat, PreviousWorkspace, StopAgent, ToggleDiff, ToggleSidebar,
};

impl AppView {
    /// Takes the shortcuts that can act now.
    pub(super) fn with_shortcuts(&self, root: Div, cx: &mut Context<Self>) -> Div {
        let connected = self.client().is_some();
        // Settings, which stands in for the sidebar, leaves these out, so there's always one to hide.
        root.on_action(cx.listener(|_, _: &ToggleSidebar, _, cx| corner::toggle_sidebar(cx)))
            .when(connected && self.current_repo().is_some(), |root| {
                root.on_action(cx.listener(|t, _: &NewWorkspace, window, cx| t.new_workspace_here(window, cx)))
            })
            .when(connected && self.hub.remote.is_none(), |root| {
                root.on_action(cx.listener(|t, _: &AddRepository, _, cx| t.add_repository(cx)))
            })
            .when(self.can_go_through_history(Way::Back), |root| {
                root.on_action(cx.listener(|t, _: &GoBack, _, cx| t.go_through_history(Way::Back, cx)))
            })
            .when(self.can_go_through_history(Way::Forward), |root| {
                root.on_action(cx.listener(|t, _: &GoForward, _, cx| t.go_through_history(Way::Forward, cx)))
            })
            .when(!self.visible_workspaces().is_empty(), |root| {
                root.on_action(cx.listener(|t, _: &NextWorkspace, _, cx| t.step_selection(1, cx)))
                    .on_action(cx.listener(|t, _: &PreviousWorkspace, _, cx| t.step_selection(-1, cx)))
            })
            // Home hides the selected workspace, so nothing acts on it unseen.
            .when(self.selected_pane().is_some() && !self.home_showing(), |root| with_workspace_shortcuts(root, cx))
            .when(self.marking_showing(cx).is_some(), |root| {
                root.on_action(cx.listener(|t, _: &MarkWorkspace, _, cx| {
                    if let Some((mark, chats)) = t.selected().and_then(|id| t.workspace_marking(id, cx)) {
                        t.mark_chats(mark, chats, cx);
                    }
                }))
            })
            .when(self.chat_showing(cx), |root| with_chat_shortcuts(root, cx))
    }

    /// What marking the workspace showing does, as its row's menu would, or `None` while there's no reply in it to mark,
    /// or Home hides it.
    fn marking_showing(&self, cx: &Context<Self>) -> Option<Mark> {
        let id = self.selected().filter(|_| self.selected_pane().is_some() && !self.home_showing())?;
        self.workspace_marking(id, cx).map(|(mark, _)| mark)
    }

    /// Whether a chat shows, in front of the selected workspace's tabs, to find text in and stop the agent of.
    fn chat_showing(&self, cx: &Context<Self>) -> bool {
        let pane = self.selected_pane().filter(|_| !self.home_showing()).map(|pane| pane.read(cx));
        pane.is_some_and(|pane| pane.showing() == Showing::Chat && pane.transcript().is_some())
    }

    /// Gives the menu bar again when what it says about the window or the selected workspace changes, such as Show
    /// Diff becoming Hide Diff, since macOS only shows what it was last given. Cheap when nothing has changed, so every
    /// frame can ask, which catches every way it can change.
    pub(super) fn keep_menus_current(&mut self, cx: &mut Context<Self>) {
        let pane = self.selected_pane().map(|pane| pane.read(cx));
        let says = menus::Says {
            sidebar_open: Side::Left.chosen(cx).sidebar == Sidebar::Open,
            diff_open: pane.is_some_and(|pane| pane.changes_open(cx)),
            showing: pane.map_or(Showing::Chat, WorkspacePane::showing),
            marking: self.marking_showing(cx),
        };
        if Some(says) != self.menus_say {
            self.menus_say = Some(says);
            // Not while the window draws.
            let view = cx.weak_entity();
            cx.defer(move |cx| {
                if view.upgrade().is_some_and(|view| view.read(cx).visible) {
                    menus::show(says, cx);
                }
            });
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

/// What acts on the chat showing, which there has to be: finding text in it, and stopping its agent. A terminal, a file
/// tab or a subagent tab in front hides the chat, so nothing takes their actions then, rather than act on a chat out of
/// sight, and off the Mac a terminal's program has their keys, as `keys.rs` leaves them out of a terminal.
fn with_chat_shortcuts(root: Div, cx: &mut Context<AppView>) -> Div {
    root.on_action(cx.listener(|t, _: &StopAgent, _, cx| t.update_selected_pane(cx, WorkspacePane::stop_agent)))
        .on_action(cx.listener(|t, _: &ConversationMode, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.start_conversation(window, cx));
        }))
        .on_action(cx.listener(|t, _: &FindInChat, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.find_in_chat(window, cx));
        }))
        .on_action(cx.listener(|t, _: &FindNext, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.find_next(true, window, cx));
        }))
        .on_action(cx.listener(|t, _: &FindPrevious, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.find_next(false, window, cx));
        }))
}

/// What acts on the selected workspace, which there has to be.
fn with_workspace_shortcuts(root: Div, cx: &mut Context<AppView>) -> Div {
    root.on_action(cx.listener(|t, _: &ToggleDiff, _, cx| t.update_selected_pane(cx, WorkspacePane::toggle_changes)))
        .on_action(cx.listener(|t, _: &CreatePullRequest, _, cx| {
            t.update_selected_pane(cx, WorkspacePane::create_pull_request);
        }))
        .on_action(cx.listener(|t, _: &NewChat, _, cx| t.update_selected_pane(cx, WorkspacePane::new_chat)))
        .on_action(cx.listener(|t, _: &NewTerminal, _, cx| t.update_selected_pane(cx, WorkspacePane::new_terminal)))
        .on_action(cx.listener(|t, _: &CloseChat, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.close_tab(window, cx));
        }))
        .on_action(cx.listener(|t, _: &NextChat, _, cx| t.update_selected_pane(cx, |pane, cx| pane.step_chat(1, cx))))
        .on_action(
            cx.listener(|t, _: &PreviousChat, _, cx| t.update_selected_pane(cx, |pane, cx| pane.step_chat(-1, cx))),
        )
        // As the selected workspace's row offers them, for the row of the workspace showing.
        .on_action(cx.listener(|t, _: &ArchiveWorkspace, _, cx| {
            if let Some(id) = t.selected().cloned() {
                t.archive(id, false, cx);
            }
        }))
        .on_action(cx.listener(|t, _: &Copy, window, cx| {
            t.update_selected_pane(cx, |pane, cx| pane.copy_showing(window, cx));
        }))
}
