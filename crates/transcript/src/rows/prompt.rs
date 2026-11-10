//! What a prompt looks like: what was typed, the files it carries, and whether the agent has it yet.

use client_core::session_log::PromptState;
use gpui_kit::component::text::TextViewState;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Entity, IntoElement, ParentElement, Styled, WeakEntity, div, px, relative};
use protocol::model::Seq;

use super::row;
use crate::attachments::{self, Carried};
use crate::item_key::ItemKey;
use crate::transcript::Transcript;

/// The widest a prompt is, as a share of the column. What was asked sits at the right, as wide as it needs, so it
/// stands apart from the agent's work, which starts at the left.
const WIDEST_SHARE: f32 = 0.8;

/// Where a prompt is on its way to the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    /// Sent, and not yet recorded, so it has no place in the log, and its images can't be opened.
    Sending,
    /// Recorded at this place in the log, with the agent, or waiting for it.
    Recorded(Seq, PromptState),
}

/// What was sent, on a bubble at the right of the column, with the files it carries above its text, in the order the
/// agent was given them. It's faint until the agent has it, so one sent mid-turn reads as not yet seen until the agent
/// takes it, and one the agent never took says so.
pub(super) fn prompt_bubble(
    key: &ItemKey,
    text: &str,
    held: Option<&Entity<TextViewState>>,
    carried: &[Carried],
    progress: Progress,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> AnyElement {
    let typed = !text.trim().is_empty();
    let (seq, state) = match progress {
        Progress::Sending => (None, None),
        Progress::Recorded(seq, state) => (Some(seq), Some(state)),
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
                .children(attachments::view(seq, carried, this, cx))
                .when(typed, |d| d.child(super::markdown(key.element("prompt"), text, held))),
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
