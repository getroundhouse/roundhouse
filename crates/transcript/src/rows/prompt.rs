//! What a prompt looks like: what was typed, the files it carries, and whether the agent has it yet.

use client_core::session_log::PromptState;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div, relative,
};
use look::{Shaped as _, Typeset as _};

use super::{Shared, from_blocks, row};
use crate::attachments::{self, Carried};
use crate::find::{FindLook, Part};
use crate::item_key::ItemKey;
use crate::texts::{TextAt, Written};

/// The widest a prompt is, as a share of the column. What was asked sits at the right, as wide as it needs, so it
/// stands apart from the agent's work, which starts at the left.
const WIDEST_SHARE: f32 = 0.8;

/// What a prompt says, what's being found in it, and where it's drawn: at the end of the chat while the agent hasn't
/// read it, and then in the row of the event that settled it, where the agent took it or dropped it.
#[derive(Clone, Copy)]
pub(super) struct Prompt<'a> {
    pub(super) text: &'a str,
    pub(super) find: &'a FindLook,
    pub(super) at: TextAt,
}

/// What was sent, on a bubble at the right of the column, with the files it carries above its text, in the order the
/// agent was given them, and where the agent has it once it's recorded, `state`, which is `None` while it's being sent.
/// It's faint until the agent has it, so one sent mid-turn reads as not yet seen until the agent takes it, and one the
/// agent never took says so.
pub(super) fn prompt_bubble(
    key: &ItemKey,
    text: Prompt,
    carried: &[Carried],
    state: Option<PromptState>,
    shared: &Shared,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let Prompt { text, find, at } = text;
    let typed = !text.trim().is_empty();
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
                .corners(look::Corner::Control)
                .bg(colors.prompt)
                .when(faint, |d| d.opacity(0.6))
                .children(attachments::view(carried, cx))
                .when(typed, |d| {
                    // Drawn from the kit's blocks, and selected by the transcript, from when it's sent.
                    let id = (key.clone(), Part::Prompt);
                    d.child(from_blocks(&Written::found(at, &id, find, text), None, shared))
                })
                .test_support(),
        )
        .when(dropped, |d| {
            d.child(
                div()
                    .typeset(look::Type::Detail)
                    .text_color(colors.text_faint)
                    .child("The agent stopped before it read this, so it never got it."),
            )
        })
        .into_any_element()
}
