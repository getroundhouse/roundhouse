//! A tool call, and the result it got back, drawn together. They arrive as separate events, often with other calls
//! in between when the agent runs several at once, so each call's result is found in the log by [`super::around`].
//! What each kind of call looks like is in the modules below, all on the same [`super::line`].

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, WeakEntity};
use protocol::model::Seq;
use serde_json::Value;

use super::around::{Answer, Reply, Subagent};
use crate::calls::{ToolCall, count};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::transcript::Transcript;

mod advisor;
pub(super) mod agent;
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
    /// The images text can point to, for a subagent's work, which is drawn in its call.
    pub(super) linked: &'a LinkedImages,
}

/// A call's line, with its result under it once it's back, as its kind draws it. Only a tool the API runs itself is
/// ever recorded as dropped, and the advisor is the one such tool drawn in a way of its own, so only it says so. Any
/// other draws as it does while it waits.
pub(super) fn call(
    toggle: &Toggle,
    name: &str,
    input: &Value,
    reply: &Reply,
    subagent: Option<&Subagent>,
    cx: &App,
) -> AnyElement {
    let answer = reply.result();
    match ToolCall::read(name, input) {
        ToolCall::Command(command) => command::draw(&command, answer, toggle, cx),
        ToolCall::Read(file) => files::read(&file, answer, toggle, cx),
        ToolCall::Write(file) => files::write(&file, answer, toggle, cx),
        ToolCall::Edit(file) => files::edit(&file, answer, toggle, cx),
        ToolCall::Search(search) => search::draw(&search, answer, toggle, cx),
        ToolCall::Agent(call) => agent::draw(&call, answer, subagent, toggle, cx),
        ToolCall::Fetch(fetch) => web::fetch(&fetch, answer, toggle, cx),
        ToolCall::WebSearch(search) => web::search(&search, answer, toggle, cx),
        ToolCall::Advisor => advisor::draw(reply, toggle, cx),
        ToolCall::Other(other) => other::draw(&other, input, answer, toggle, cx),
    }
}

/// How much came back, such as `10 lines`, `1 image`, or both, or `None` for nothing.
pub(super) fn amount(lines: usize, images: usize) -> Option<String> {
    let lines = (lines > 0).then(|| count(lines, "line", "lines"));
    let images = (images > 0).then(|| count(images, "image", "images"));
    match (lines, images) {
        (Some(lines), Some(images)) => Some(format!("{lines} and {images}")),
        (lines, images) => lines.or(images),
    }
}

/// A result drawn on its own, because its call isn't near it: a line saying how much came back, which opens to it.
pub(super) fn result(toggle: &Toggle, answer: &Answer, cx: &App) -> AnyElement {
    let (text, images) = (answer.text_beside_images(), answer.images());
    let lines = text.map_or(0, |text| text.lines().count());
    let size = amount(lines, images.len()).unwrap_or_else(|| count(0, "line", "lines"));
    let line = Line::new(IconName::CornerDownRight, "Output").part(Part::Note(size.into())).shows_output(true);
    let opened = toggle.open.then(|| {
        let color = cx.theme().muted_foreground;
        block::frame(cx)
            .when_some(text, |d, text| d.child(block::output(toggle.key.element("output"), text, color, cx)))
            .when(!images.is_empty(), |d| d.child(block::images(images, cx)))
            .into_any_element()
    });
    line::drawn(line, Some(answer), opened, toggle, cx)
}

#[cfg(test)]
mod tests {
    use super::amount;

    #[test]
    fn how_much_came_back_says_lines_images_or_both() {
        assert_eq!(amount(0, 0), None);
        assert_eq!(amount(10, 0).as_deref(), Some("10 lines"));
        assert_eq!(amount(0, 1).as_deref(), Some("1 image"));
        assert_eq!(amount(1, 2).as_deref(), Some("1 line and 2 images"));
    }
}
