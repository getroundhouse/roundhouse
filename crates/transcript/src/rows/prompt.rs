//! What a prompt looks like: what was typed, the files it carries, and whether the agent has it yet.

use client_core::session_log::PromptState;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div, px,
    relative,
};
use protocol::model::Seq;

use super::{Shared, recorded, row};
use crate::attachments::{self, Carried};
use crate::find::{self, FindLook, Part};
use crate::item_key::ItemKey;
use crate::prose::{TextAt, Written};

/// The widest a prompt is, as a share of the column. What was asked sits at the right, as wide as it needs, so it
/// stands apart from the agent's work, which starts at the left.
const WIDEST_SHARE: f32 = 0.8;

/// Where a prompt is on its way to the agent, and where it's drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    /// Sent, and not yet recorded, so it has no place in the log.
    Sending,
    /// Recorded, and drawn at the end of the chat, since the agent hasn't read it yet, so it's in no event's row.
    Waiting(PromptState),
    /// Recorded, and drawn in the row of the event at this place in the log: its own, or the one that settled it,
    /// where the agent took it or dropped it.
    InRow(Seq, PromptState),
}

impl Progress {
    /// A recorded prompt's, drawn in the row of the event at `place` in the log, or at the end of the chat for `None`.
    pub(crate) fn recorded(place: Option<Seq>, state: PromptState) -> Self {
        match place {
            Some(place) => Progress::InRow(place, state),
            None => Progress::Waiting(state),
        }
    }
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
    // Sent, or waiting at the end of the chat, it's the kit's text view, with the kit's selection.
    let (place, state) = match progress {
        Progress::Sending => (None, None),
        Progress::Waiting(state) => (None, Some(state)),
        Progress::InRow(place, state) => (Some(place), Some(state)),
    };
    let faint = state != Some(PromptState::Taken);
    let dropped = state == Some(PromptState::Dropped);
    row()
        .items_end()
        .child(
            v_flex()
                .id(key.element("bubble"))
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
                    d.child(match place {
                        // In a row, the transcript draws it from the kit's blocks, and selects it itself, with the
                        // view it was drawn by while it was sent until the record has a parse of its own.
                        Some(place) => recorded(&Written::found(TextAt::only(place, 0), &id, find, text), None, shared),
                        None => find::markdown_view(shared.texts.markdown(&id, text), &id, find, None),
                    })
                })
                .test_support(),
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
