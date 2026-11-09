//! A tool call, and the result it got back, drawn together. They arrive as separate events, often with other calls
//! in between when the agent runs several at once, so each call's result is found in the log by [`super::around`].
//! What each kind of call looks like is in the modules below, all on the same [`super::line`].

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, WeakEntity};
use protocol::model::Seq;
use serde_json::Value;

use super::around::{Answer, Subagent};
use crate::calls::{ToolCall, count};
use crate::item_key::ItemKey;
use crate::transcript::Transcript;

mod agent;
pub(super) mod block;
mod command;
mod files;
mod highlight;
mod numbered;
mod other;
mod search;
mod web;

use super::line::{self, Line, Part};

/// Where a call is drawn and whether it's open, which its line needs to open and close it.
pub(super) struct Toggle<'a> {
    pub(super) seq: Seq,
    pub(super) key: &'a ItemKey,
    pub(super) open: bool,
    pub(super) this: &'a WeakEntity<Transcript>,
}

/// A call's line, with its result under it once it's back, as its kind draws it.
pub(super) fn call(
    toggle: &Toggle,
    name: &str,
    input: &Value,
    answer: Option<&Answer>,
    subagent: Option<&Subagent>,
    cx: &App,
) -> AnyElement {
    match ToolCall::read(name, input) {
        ToolCall::Command(command) => command::draw(&command, answer, toggle, cx),
        ToolCall::Read(file) => files::read(&file, answer, toggle, cx),
        ToolCall::Write(file) => files::write(&file, answer, toggle, cx),
        ToolCall::Edit(file) => files::edit(&file, answer, toggle, cx),
        ToolCall::Search(search) => search::draw(&search, answer, toggle, cx),
        ToolCall::Agent(call) => agent::draw(&call, answer, subagent, toggle, cx),
        ToolCall::Fetch(fetch) => web::fetch(&fetch, answer, toggle, cx),
        ToolCall::WebSearch(search) => web::search(&search, answer, toggle, cx),
        ToolCall::Other(other) => other::draw(&other, input, answer, toggle, cx),
    }
}

/// A result drawn on its own, because its call isn't near it: a line saying how much came back, which opens to it.
pub(super) fn result(toggle: &Toggle, is_error: bool, text: &str, cx: &App) -> AnyElement {
    let answer = &Answer { is_error, text: text.to_string(), change: None };
    let lines = count(answer.text.lines().count(), "line", "lines");
    let line = Line::new(IconName::CornerDownRight, "Output").part(Part::Note(lines.into())).shows_output(true);
    let opened = toggle.open.then(|| {
        let output = block::output(toggle.key.element("output"), &answer.text, cx.theme().muted_foreground, cx);
        block::frame(cx).child(output).into_any_element()
    });
    line::drawn(line, Some(answer), opened, toggle, cx)
}
