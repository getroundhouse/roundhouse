//! What a prompt looks like: what was typed, the files it carries, and whether the agent has it yet.

use client_core::session_log::PromptState;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, div};

use super::row;
use crate::item_key::ItemKey;

/// Where a prompt is on its way to the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    /// Sent, and not yet recorded.
    Sending,
    /// Recorded, with the agent, or waiting for it.
    Recorded(PromptState),
}

/// What was sent, with the files it carries above its text, in the order the agent was given them. It's faint until
/// the agent has it, so one sent mid-turn reads as not yet seen until the agent takes it, and one the agent never took
/// says so.
pub(super) fn prompt_bubble(
    key: &ItemKey,
    text: String,
    carried: Option<AnyElement>,
    progress: Progress,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let typed = !text.trim().is_empty();
    let faint = !matches!(progress, Progress::Recorded(PromptState::Taken));
    let dropped = progress == Progress::Recorded(PromptState::Dropped);
    row()
        .child(
            v_flex()
                .w_full()
                .gap_2p5()
                .px_3p5()
                .py_2p5()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .when(faint, |d| d.opacity(0.6))
                .children(carried)
                .when(typed, |d| {
                    d.child(TextView::markdown(key.element("prompt"), SharedString::from(text)).selectable(true))
                }),
        )
        .when(dropped, |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(look::faint())
                    .child("The agent stopped before it read this, so it never got it."),
            )
        })
        .into_any_element()
}
