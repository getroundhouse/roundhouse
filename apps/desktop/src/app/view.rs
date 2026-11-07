//! How the app looks, and the shortcuts it answers.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};

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
            Connection::Failed(reason) => centered(&format!("Couldn't reach the hub. {reason}"), danger),
            Connection::OtherVersion(reason) => other_version(reason.clone(), muted, cx),
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

fn other_version(reason: String, muted: Hsla, cx: &mut Context<AppView>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child("The running hub is a different version from this app.")
        .child(div().max_w(px(560.0)).text_sm().text_color(muted).child(reason))
        .child(
            Button::new("restart-hub")
                .primary()
                .label("Restart the hub")
                .on_click(cx.listener(|t, _, _, cx| t.restart_hub(cx))),
        )
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
