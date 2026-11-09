//! A shell command: what the agent said it's for, with the command beside it. Opened, the command and then its
//! output, in one block.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement};

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::calls::{Command, failure};

pub(super) fn draw(command: &Command, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| Part::Pill(command.summary().to_string().into()));
    let line = Line::new(IconName::Terminal, command.title()).part(summary);
    let opened = toggle.open.then(|| opened(command, answer, toggle, cx).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}

fn opened(command: &Command, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> Div {
    let shown = block::code(toggle.key.element("command"), "sh", "$ ", command.command, cx);
    let output = answer.map(|a| if a.is_error { failure::output(&a.text) } else { a.text.as_str() });
    block::frame(cx).child(shown).when_some(output.filter(|text| !text.trim().is_empty()), |d, text| {
        let id = toggle.key.element("output");
        d.child(block::separator(cx)).child(block::output(id, text, cx.theme().muted_foreground, cx))
    })
}
