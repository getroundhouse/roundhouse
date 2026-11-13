//! Any tool without a drawing of its own, such as one from an MCP server. Opened, its input as JSON, then what came
//! back.

use client_core::calls::Other;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement};
use serde_json::Value;

use super::line::{Line, Part};
use super::{Answer, Outcome, Toggle, block, line};
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

pub(super) fn draw(other: &Other, input: &Value, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let icon = if other.server.is_some() { IconName::Plug } else { IconName::Wrench };
    let detail = other.detail.clone().filter(|_| !toggle.open);
    let line = Line::new(icon, other.title.clone())
        .part(other.server.clone().map(|server| Part::Chip(server.into())))
        .part(other.tool.clone().map(|tool| Part::Note(tool.into())))
        .part(detail.map(|detail| Part::Code(detail.into())))
        .shows_output(true);
    // With no description, the title is the tool's name made readable, which is the app's.
    let line = if other.tool.is_some() { line.found(toggle) } else { line };
    let opened = toggle.open.then(|| opened(input, answer, toggle, cx).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(other: &Other, input: &Value, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    if other.tool.is_some() {
        gather.plain(key, Text::Title, &other.title);
    }
    gather.inside(key, Fold::Output, |gather| {
        gather.plain(key, Text::Input, block::code_text(&json(input)));
        if let Some(text) = came_back(answer).and_then(Answer::text_beside_images) {
            gather.plain(key, Text::Output, &block::output_text(text));
        }
    });
}

/// What came back, unless the person denied the call, which sent nothing back of its own, and whose line says so.
fn came_back(answer: Option<&Answer>) -> Option<&Answer> {
    answer.filter(|answer| !matches!(answer.outcome(), Outcome::Denied(_)))
}

/// Its input, as it's shown.
fn json(input: &Value) -> String {
    serde_json::to_string_pretty(input).unwrap_or_default()
}

fn opened(input: &Value, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> Div {
    block::frame(cx)
        .child(block::label("Input", cx))
        .child(block::code(toggle, Text::Input, "json", "", &json(input), cx))
        .when_some(came_back(answer), |d, answer| {
            let color = if answer.failure().is_some() { cx.theme().danger } else { cx.theme().foreground };
            let images = answer.images();
            d.child(block::separator(cx))
                .child(block::label("Output", cx))
                .when_some(answer.text_beside_images(), |d, text| {
                    d.child(block::output(toggle, Text::Output, text, color, cx))
                })
                .when(!images.is_empty(), |d| d.child(block::images(images, cx)))
        })
}
