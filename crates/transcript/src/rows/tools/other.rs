//! Any tool without a drawing of its own, such as one from an MCP server. Opened, its input as JSON, then what came
//! back.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement};
use serde_json::Value;

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::calls::Other;

pub(super) fn draw(other: &Other, input: &Value, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let icon = if other.server.is_some() { IconName::Plug } else { IconName::Wrench };
    let detail = other.detail.clone().filter(|_| !toggle.open);
    let line = Line::new(icon, other.title.clone())
        .part(other.server.clone().map(|server| Part::Chip(server.into())))
        .part(other.tool.clone().map(|tool| Part::Note(tool.into())))
        .part(detail.map(|detail| Part::Code(detail.into())))
        .shows_output(true);
    let opened = toggle.open.then(|| opened(input, answer, toggle, cx).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}

fn opened(input: &Value, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> Div {
    let json = serde_json::to_string_pretty(input).unwrap_or_default();
    block::frame(cx)
        .child(block::label("Input", cx))
        .child(block::code(toggle.key.element("input"), "json", "", &json, cx))
        .when_some(answer, |d, answer| {
            let color = if answer.is_error { cx.theme().danger } else { cx.theme().foreground };
            let output = block::output(toggle.key.element("output"), &answer.text, color, cx);
            d.child(block::separator(cx)).child(block::label("Output", cx)).child(output)
        })
}
