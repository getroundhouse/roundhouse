use super::super::around::{Answer, Reply, Subagent};
use super::super::line::{Line, Part};
use super::super::shown::Gather;
use super::super::{InRow, Shared, line};
use super::{advisor, agent, block, command, files, other, search, web};
use crate::find::{FindLook, Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use client_core::calls::{Recorded, ToolCall, count};
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement};
use protocol::model::Seq;

/// Where a call is drawn and whether it's open, which its line needs to open and close it, and what every row is drawn
/// with, which a subagent's work, drawn in its call, needs too.
pub(in super::super) struct Toggle<'a> {
    pub(in super::super) seq: Seq,
    pub(in super::super) key: &'a ItemKey,
    pub(in super::super) fold: Fold,
    pub(in super::super) open: bool,
    pub(in super::super) shared: &'a Shared,
    /// What's being found, which the call draws the matches of.
    pub(in super::super) find: &'a FindLook,
    /// The row it's drawn in, which says where its texts are placed among those that row draws.
    pub(in super::super) in_row: InRow,
}

impl Toggle<'_> {
    /// Where the matches are in `text`, which is the call's `part` as it's drawn.
    pub(in super::super) fn marks(&self, part: Text, text: &str) -> Marks {
        self.find.marks(&(self.key.clone(), part), text)
    }
}

/// A call's line, with its result under it once it's back, as its kind draws it. Only a tool the API runs itself is
/// ever recorded as dropped, and the advisor is the one such tool drawn in a way of its own, so only it says so. Any
/// other draws as it does while it waits.
pub(in super::super) fn call(
    toggle: &Toggle,
    recorded: Recorded<'_>,
    reply: &Reply,
    subagent: Option<&Subagent>,
    cx: &App,
) -> AnyElement {
    let (answer, input) = (reply.result(), recorded.input);
    match ToolCall::read(recorded) {
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

/// What a call shows, as [`call`] draws it open, for finding.
pub(in super::super) fn shown(
    key: &ItemKey,
    recorded: Recorded<'_>,
    reply: &Reply,
    subagent: Option<&Subagent>,
    gather: &mut Gather,
) {
    let (answer, input) = (reply.result(), recorded.input);
    match ToolCall::read(recorded) {
        ToolCall::Command(command) => command::shown(&command, answer, key, gather),
        ToolCall::Read(file) => files::shown_read(&file, answer, key, gather),
        ToolCall::Write(file) => files::shown_write(&file, answer, key, gather),
        ToolCall::Edit(file) => files::shown_edit(&file, answer, key, gather),
        ToolCall::Search(search) => search::shown(&search, answer, key, gather),
        ToolCall::Agent(call) => agent::shown(&call, answer, subagent, key, gather),
        ToolCall::Fetch(fetch) => web::shown_fetch(&fetch, answer, key, gather),
        ToolCall::WebSearch(search) => web::shown_search(&search, answer, key, gather),
        ToolCall::Advisor => advisor::shown(reply, key, gather),
        ToolCall::Other(other) => other::shown(&other, input, answer, key, gather),
    }
    line::shown_denial(answer, key, gather);
}

/// How much came back, such as `10 lines`, `1 image`, or both, or `None` for nothing.
pub(in super::super) fn amount(lines: usize, images: usize) -> Option<String> {
    let lines = (lines > 0).then(|| count(lines, "line", "lines"));
    let images = (images > 0).then(|| count(images, "image", "images"));
    match (lines, images) {
        (Some(lines), Some(images)) => Some(format!("{lines} and {images}")),
        (lines, images) => lines.or(images),
    }
}

/// A result drawn on its own, because its call isn't near it: a line saying how much came back, which opens to it.
pub(in super::super) fn result(toggle: &Toggle, answer: &Answer, cx: &App) -> AnyElement {
    let (text, images) = (answer.text_beside_images(), answer.images());
    let lines = text.map_or(0, |text| text.lines().count());
    let size = amount(lines, images.len()).unwrap_or_else(|| count(0, "line", "lines"));
    let line = Line::new(IconName::CornerDownRight, "Output").part(Part::Note(size.into())).shows_output(true);
    let opened = || {
        let color = look::colors(cx).text_muted;
        let shown = block::frame(cx)
            .when_some(text, |d, text| d.child(block::output(toggle, Text::Output, text, color, cx)))
            .when(!images.is_empty(), |d| d.child(block::images(images, cx)));
        Some(shown.into_any_element())
    };
    line::drawn(line, Some(answer), opened, toggle, cx)
}

/// What [`result`] shows open, for finding.
pub(in super::super) fn shown_result(key: &ItemKey, answer: &Answer, gather: &mut Gather) {
    if let Some(text) = answer.text_beside_images() {
        gather.inside(key, Fold::Output, |gather| gather.plain(key, Text::Output, &block::output_text(text)));
    }
    line::shown_denial(Some(answer), key, gather);
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
