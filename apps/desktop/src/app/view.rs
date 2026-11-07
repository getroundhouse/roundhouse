//! How the app looks, and the shortcuts it answers.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use protocol::client::HubStatus;

use super::{AppView, Connection};
use crate::workspace_pane::WorkspacePane;
use crate::{
    AddRepository, CloseChat, CreatePullRequest, NewChat, NewWorkspace, NextChat, NextWorkspace, PreviousChat,
    PreviousWorkspace, StopAgent, ToggleDiff,
};

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Shortcuts only reach this view through whatever has focus, and focus can end up nowhere, as
        // when what had it goes away. Every shortcut would then do nothing until something is clicked.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let theme = cx.theme();
        let (background, foreground, muted, danger) =
            (theme.background, theme.foreground, theme.muted_foreground, theme.danger);
        let main: AnyElement = match &self.connection {
            Connection::Connecting => centered("Connecting…", muted),
            Connection::Failed(reason) => failed(reason, danger, cx),
            Connection::AcrossABreak(status) => across_a_break(status, muted, cx),
            Connection::Ready(_) => match self.selected_pane() {
                Some(pane) => pane.into_any_element(),
                None if self.repos.is_empty() => centered("Add a repository to get started.", muted),
                None => centered("Create a workspace to get started.", muted),
            },
        };
        with_actions(h_flex(), cx)
            .id("app")
            .track_focus(&self.focus)
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(crate::sidebar::render(self, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when_some(self.error.clone(), |d, error| d.child(error_bar(error, danger, cx)))
                    .child(div().flex_1().min_h_0().child(main)),
            )
    }
}

fn with_actions(root: Div, cx: &mut Context<AppView>) -> Div {
    root.on_action(cx.listener(|t, _: &NewWorkspace, _, cx| {
        if let Some(repo) = t.current_repo() {
            t.new_workspace(repo, cx);
        }
    }))
    .on_action(cx.listener(|t, _: &AddRepository, _, cx| t.add_repository(cx)))
    .on_action(cx.listener(|t, _: &NextWorkspace, _, cx| t.step_selection(1, cx)))
    .on_action(cx.listener(|t, _: &PreviousWorkspace, _, cx| t.step_selection(-1, cx)))
    .on_action(cx.listener(|t, _: &ToggleDiff, _, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, WorkspacePane::toggle_diff);
        }
    }))
    .on_action(cx.listener(|t, _: &CreatePullRequest, _, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, WorkspacePane::create_pull_request);
        }
    }))
    .on_action(cx.listener(|t, _: &NewChat, _, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, WorkspacePane::new_chat);
        }
    }))
    .on_action(cx.listener(|t, _: &CloseChat, window, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, |p, cx| {
                if let Some(id) = p.active.clone() {
                    p.close_chat(id, window, cx);
                }
            });
        }
    }))
    .on_action(cx.listener(|t, _: &NextChat, _, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, |p, cx| p.step_chat(1, cx));
        }
    }))
    .on_action(cx.listener(|t, _: &PreviousChat, _, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, |p, cx| p.step_chat(-1, cx));
        }
    }))
    .on_action(cx.listener(|t, _: &StopAgent, _, cx| {
        if let Some(pane) = t.selected_pane() {
            pane.update(cx, WorkspacePane::stop_agent);
        }
    }))
}

fn error_bar(error: String, danger: Hsla, cx: &mut Context<AppView>) -> impl IntoElement {
    h_flex()
        .id("error-bar")
        .flex_none()
        .px_3()
        .py_2()
        .gap_2()
        .bg(danger.opacity(0.15))
        .text_sm()
        .text_color(danger)
        .cursor_pointer()
        .on_click(cx.listener(|t, _, _, cx| {
            t.error = None;
            cx.notify();
        }))
        .child(div().flex_1().min_w_0().child(error))
        .child(div().flex_none().child("Dismiss"))
}

/// What restarting would cost, since this is the one screen that can stop agents with one click.
fn restart_note(agents: u32, setups: u32) -> String {
    let count = |n: u32, what: &str| match n {
        0 => None,
        1 => Some(format!("1 {what}")),
        n => Some(format!("{n} {what}s")),
    };
    let start = "Restarting it starts one that matches";
    match (count(agents, "agent"), count(setups, "setup script")) {
        (None, None) => format!("{start}. Nothing is running on it."),
        (Some(both), None) | (None, Some(both)) => format!("{start}, and stops the {both} running on it."),
        (Some(agents), Some(setups)) => format!("{start}, and stops the {agents} and {setups} running on it."),
    }
}

/// Only a hub a client started, and only an older one. This app's own hub might not be able to open what a newer
/// one has written, so restarting that would stop its agents and leave nothing running.
fn can_restart(status: &HubStatus) -> bool {
    status.on_demand && status.protocol_version < protocol::PROTOCOL_VERSION
}

fn try_again(cx: &mut Context<AppView>) -> Button {
    Button::new("try-hub-again").label("Try again").on_click(cx.listener(|t, _, _, cx| t.connect(cx)))
}

fn failed(reason: &str, danger: Hsla, cx: &mut Context<AppView>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div().max_w(px(560.0)).text_center().text_color(danger).child(format!("Couldn't reach the hub. {reason}")),
        )
        .child(try_again(cx))
        .into_any_element()
}

fn across_a_break(status: &HubStatus, muted: Hsla, cx: &mut Context<AppView>) -> AnyElement {
    let versions = format!("The hub is version {}, and this app is {}.", status.hub_version, protocol::BUILD_VERSION);
    let next = if can_restart(status) {
        restart_note(status.running_sessions, status.running_setups)
    } else if status.on_demand {
        "It's newer than this app, so the app needs updating.".to_string()
    } else {
        "It was started by hand, so it's yours to update or stop. To stop it and its agents, run \"trestle hub stop\"."
            .to_string()
    };
    let note = |text: String| div().max_w(px(560.0)).text_center().text_sm().text_color(muted).child(text);
    let action = if can_restart(status) {
        Button::new("restart-hub")
            .primary()
            .label("Restart the hub")
            .on_click(cx.listener(|t, _, _, cx| t.restart_hub(cx)))
    } else {
        try_again(cx)
    };
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child("This app can't talk to the running hub.")
        .child(note(versions))
        .child(note(next))
        .child(action)
        .into_any_element()
}

fn centered(text: &str, color: Hsla) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(color)
        .child(text.to_string())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_restart_note_says_what_would_stop() {
        assert!(restart_note(0, 0).ends_with("Nothing is running on it."));
        assert!(restart_note(1, 0).ends_with("stops the 1 agent running on it."));
        assert!(restart_note(2, 0).ends_with("stops the 2 agents running on it."));
        assert!(restart_note(0, 2).ends_with("stops the 2 setup scripts running on it."));
        assert!(restart_note(2, 1).ends_with("stops the 2 agents and 1 setup script running on it."));
    }
}
