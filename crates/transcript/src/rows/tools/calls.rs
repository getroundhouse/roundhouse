use super::super::around::{Answer, BlockAround};
use super::super::line::{Line, Part};
use super::super::shown::Gather;
use super::super::{InRow, Shared, line};
use super::{advisor, agent, block, command, files, image, notebook, other, patch, search, to_subagents, web};
use crate::find::{FindLook, Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use client_core::calls::{Command, Only, ReadFile, Recorded, ToolCall, count};
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

/// A call's line, with its result under it once it's back, as its kind draws it, from what's been read around it,
/// `drawing`. Only a tool the API runs itself is ever recorded as dropped, and the advisor is the one such tool drawn
/// in a way of its own, so only it says so. Any other draws as it does while it waits.
pub(in super::super) fn call(toggle: &Toggle, recorded: Recorded<'_>, drawing: &BlockAround, cx: &App) -> AnyElement {
    let (reply, subagent) = (&drawing.reply, drawing.subagent.as_ref());
    let (answer, input) = (reply.result(), recorded.input());
    match ToolCall::read(recorded) {
        ToolCall::Command(command) => match only(&command, answer) {
            Some(Only::Reads(path)) => files::read(&ReadFile { path, lines: None }, answer, toggle, cx),
            Some(Only::Searches(search)) => search::draw(&search, answer, toggle, cx),
            None => command::draw(&command, answer, toggle, cx),
        },
        ToolCall::Read(file) => files::read(&file, answer, toggle, cx),
        ToolCall::Write(file) => files::write(&file, answer, toggle, cx),
        ToolCall::Edit(file) => files::edit(&file, answer, toggle, cx),
        ToolCall::Search(search) => search::draw(&search, answer, toggle, cx),
        ToolCall::Agent(call) => agent::draw(&call, answer, subagent, toggle, cx),
        ToolCall::ToSubagents(to) => to_subagents::draw(&to, answer, drawing.names.as_ref(), toggle, cx),
        ToolCall::Fetch(fetch) => web::fetch(&fetch, answer, toggle, cx),
        ToolCall::WebSearch(search) => web::search(&search, answer, toggle, cx),
        ToolCall::Advisor => advisor::draw(reply, toggle, cx),
        ToolCall::Patch(patch) => patch::draw(patch, answer, toggle, cx),
        ToolCall::NotebookEdit(edit) => notebook::draw(edit, answer, toggle, cx),
        ToolCall::MakeImage(made) => image::draw(made, answer, toggle, cx),
        ToolCall::Typed(call) => super::typed::draw(call, answer, toggle, cx),
        ToolCall::Other(other) => other::draw(&other, input, answer, toggle, cx),
    }
}

/// What a call shows, as [`call`] draws it open, for finding.
pub(in super::super) fn shown(key: &ItemKey, recorded: Recorded<'_>, drawing: &BlockAround, gather: &mut Gather) {
    let (reply, subagent) = (&drawing.reply, drawing.subagent.as_ref());
    let (answer, input) = (reply.result(), recorded.input());
    match ToolCall::read(recorded) {
        ToolCall::Command(command) => match only(&command, answer) {
            Some(Only::Reads(path)) => files::shown_read(&ReadFile { path, lines: None }, answer, key, gather),
            Some(Only::Searches(search)) => search::shown(&search, answer, key, gather),
            None => command::shown(&command, answer, key, gather),
        },
        ToolCall::Read(file) => files::shown_read(&file, answer, key, gather),
        ToolCall::Write(file) => files::shown_write(&file, answer, key, gather),
        ToolCall::Edit(file) => files::shown_edit(&file, answer, key, gather),
        ToolCall::Search(search) => search::shown(&search, answer, key, gather),
        ToolCall::Agent(call) => agent::shown(&call, answer, subagent, key, gather),
        ToolCall::ToSubagents(to) => to_subagents::shown(&to, answer, key, gather),
        ToolCall::Fetch(fetch) => web::shown_fetch(&fetch, answer, key, gather),
        ToolCall::WebSearch(search) => web::shown_search(&search, answer, key, gather),
        ToolCall::Advisor => advisor::shown(reply, key, gather),
        ToolCall::Patch(patch) => patch::shown(patch, answer, key, gather),
        ToolCall::NotebookEdit(edit) => notebook::shown(edit, answer, key, gather),
        ToolCall::MakeImage(made) => image::shown(made, answer, key, gather),
        ToolCall::Typed(call) => super::typed::shown(call, answer, key, gather),
        ToolCall::Other(other) => other::shown(&other, input, answer, key, gather),
    }
    line::shown_denial(answer, key, gather);
}

/// All a command does, where it's drawn as that kind of call: reading a file, or searching. One that went on in the
/// background is drawn as a command, since only a command's row says that.
fn only<'a>(command: &Command<'a>, answer: Option<&Answer>) -> Option<Only<'a>> {
    command.only.filter(|_| !answer.is_some_and(Answer::in_the_background))
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
