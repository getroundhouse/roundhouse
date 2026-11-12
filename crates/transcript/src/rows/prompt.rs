//! What a prompt looks like: what was typed, the files it carries, and whether the agent has it yet.

use client_core::session_log::PromptState;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled, div, px, relative};

use super::{Shared, row};
use crate::attachments::{self, Carried};
use crate::find::{self, FindLook, Part};
use crate::item_key::ItemKey;

/// The widest a prompt is, as a share of the column. What was asked sits at the right, as wide as it needs, so it
/// stands apart from the agent's work, which starts at the left.
const WIDEST_SHARE: f32 = 0.8;

/// Where a prompt is on its way to the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    /// Sent, and not yet recorded, so it has no place in the log.
    Sending,
    /// Recorded, with the agent, or waiting for it.
    Recorded(PromptState),
}

/// What a prompt says, and what's being found in it.
#[derive(Clone, Copy)]
pub(super) struct Prompt<'a> {
    pub(super) text: &'a str,
    pub(super) find: &'a FindLook,
}

/// What was sent, on a bubble at the right of the column, with the files it carries above its text, in the order the
/// agent was given them. It's faint until the agent has it, so one sent mid-turn reads as not yet seen until the agent
/// takes it, and one the agent never took says so.
pub(super) fn prompt_bubble(
    key: &ItemKey,
    text: Prompt,
    carried: &[Carried],
    progress: Progress,
    shared: &Shared,
    cx: &App,
) -> AnyElement {
    let Prompt { text, find } = text;
    let typed = !text.trim().is_empty();
    let state = match progress {
        Progress::Sending => None,
        Progress::Recorded(state) => Some(state),
    };
    let faint = state != Some(PromptState::Taken);
    let dropped = state == Some(PromptState::Dropped);
    row()
        .items_end()
        .child(
            v_flex()
                .max_w(relative(WIDEST_SHARE))
                .gap_2p5()
                .px_4()
                .py_3()
                .rounded(px(6.))
                .bg(look::prompt())
                .when(faint, |d| d.opacity(0.6))
                .children(attachments::view(carried, cx))
                .when(typed, |d| {
                    let id = (key.clone(), Part::Prompt);
                    let view = shared.texts.markdown(find::view_id(&id), text);
                    d.child(find::markdown_view(view, &id, find, None))
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
