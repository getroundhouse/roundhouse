//! Reading, writing and editing a file: what the call did and to which file, with how many lines where that helps.
//! Opened, the file's lines with their numbers, and a change as a diff. Hovering an edit shows its diff, which scrolls
//! both ways.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, px,
};

use client_core::Given;
use client_core::calls::files::{file_name, numbered_lines};
use client_core::calls::{EditFile, ReadFile, WriteFile, count};
use look::{InRegion as _, Region, language};

use super::line::{Line, Part, Preview};
use super::{Answer, Toggle, amount, block, line, marks, numbered};
use crate::calls::diff;
use crate::calls::lines::{Mark, NumberedLine, Section};
use crate::find::{Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;
use crate::scroll_area::holding_wide;

const PREVIEW_WIDTH: f32 = 560.0;
/// A dozen lines and half of the next, so a preview never covers much of the transcript, and a longer diff shows that
/// it scrolls.
const PREVIEW_HEIGHT: f32 = 260.0;

pub(super) fn read(file: &ReadFile, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let read = Read::of(answer);
    let title = amount(read.lines.len(), read.images.len()).map_or_else(|| "Read".to_string(), |n| format!("Read {n}"));
    let line = Line::new(IconName::FileText, title).part(chip(file.path));
    let output = |text| block::output(toggle, Text::Output, text, cx.theme().muted_foreground, cx);
    let opened = if !toggle.open {
        None
    } else if read.lines.is_empty() && read.images.is_empty() {
        read.text.map(|text| block::frame(cx).child(output(text)).into_any_element())
    } else {
        // Its lines, or what else came back, then any images, such as a notebook's.
        let shown = block::frame(cx)
            .child(block::found_label(toggle, Text::Label, file.path, cx))
            .when(!read.lines.is_empty(), |d| {
                d.child(numbered::lines(toggle, Text::File, language(file.path), &[read.section()], cx))
            })
            .when_some(read.text, |d, text| d.child(output(text)))
            .when(!read.images.is_empty(), |d| d.child(block::images(read.images, cx)));
        Some(shown.into_any_element())
    };
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`read`] shows open, for finding.
pub(super) fn shown_read(file: &ReadFile, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    let read = Read::of(answer);
    line::shown_failure(answer, false, key, gather);
    gather.inside(key, Fold::Output, |gather| {
        if !read.lines.is_empty() || !read.images.is_empty() {
            gather.plain(key, Text::Label, file.path);
        }
        if !read.lines.is_empty() {
            gather.plain(key, Text::File, &numbered::shown_text(&[read.section()]));
        }
        if let Some(text) = read.text {
            gather.plain(key, Text::Output, &block::output_text(text));
        }
    });
}

/// What a read sent back that it shows.
struct Read<'a> {
    /// The file's lines, with their numbers.
    lines: Vec<(u32, &'a str)>,
    images: &'a [crate::attachments::Carried],
    /// What came back that isn't the file's lines, if it isn't blank. A read with nothing to show opens to nothing, as
    /// one of an image recorded before the hub kept a tool's images does. A failure is shown under the line.
    text: Option<&'a str>,
}

impl<'a> Read<'a> {
    fn of(answer: Option<&'a Answer>) -> Self {
        let lines = answer.and_then(Answer::success).map(numbered_lines).unwrap_or_default();
        let images = answer.map(Answer::images).unwrap_or_default();
        let text = answer.and_then(Answer::success).filter(|text| lines.is_empty() && !text.trim().is_empty());
        Self { lines, images, text }
    }

    fn section(&self) -> Section {
        self.lines.iter().map(|(number, text)| NumberedLine::same(*number, text)).collect()
    }
}

pub(super) fn write(file: &WriteFile, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    // A write over a file that was there is a change, counted like an edit. Otherwise all of it is new.
    let replaced = answer.and_then(Answer::hunks);
    let icon = if replaced.is_some() { IconName::FilePen } else { IconName::FilePlus };
    let title = format!("Write {}", count(file.content.lines().count(), "line", "lines"));
    let line = Line::new(icon, title)
        .part(chip(file.path))
        .part(replaced.map(|hunks| Part::Counts(diff::change_counts(hunks))));
    let opened =
        toggle.open.then(|| shown_file(toggle, file.path, &marks::marked(&Rc::new(written(file, answer))), cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`write`] shows open, for finding.
pub(super) fn shown_write(file: &WriteFile, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    shown_lines(file.path, &written(file, answer), answer, key, gather);
}

/// What a write did: changed a file that was there, or made all of a new one.
fn written(file: &WriteFile, answer: Option<&Answer>) -> Vec<Section> {
    answer.and_then(Answer::hunks).map_or_else(|| vec![whole(file.content)], diff::from_hunks)
}

pub(super) fn edit(file: &EditFile, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let sections = Rc::new(edited(file, answer));
    let line = Line::new(IconName::FilePen, "Edit")
        .part(chip(file.path))
        .part(Part::Counts(diff::counts(&sections)))
        .preview(preview(language(file.path), sections.clone()));
    let opened = toggle.open.then(|| shown_file(toggle, file.path, &marks::marked(&sections), cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`edit`] shows open, for finding.
pub(super) fn shown_edit(file: &EditFile, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    shown_lines(file.path, &edited(file, answer), answer, key, gather);
}

/// What an edit did, from the patch the agent sent back, or worked out from the call before it has.
fn edited(file: &EditFile, answer: Option<&Answer>) -> Vec<Section> {
    answer
        .and_then(Answer::hunks)
        .map_or_else(|| file.edits.iter().map(|(old, new)| diff::from_text(old, new)).collect(), diff::from_hunks)
}

/// What an edit or a write would do to its file, worked out from what it was given, for where it's asked about before
/// it's made.
pub(crate) struct ChangeAsked {
    pub(crate) path: String,
    sections: Rc<Vec<Section>>,
}

impl ChangeAsked {
    /// What `given` would change, if it's an edit or a write.
    pub(crate) fn of(given: &Given) -> Option<Self> {
        let (path, sections) = match given {
            Given::Edit(edit) => (edit.path, edited(edit, None)),
            Given::Write(write) => (write.path, written(write, None)),
            Given::Command(_) | Given::MainInput(_) => return None,
        };
        Some(Self { path: path.to_string(), sections: marks::marked(&Rc::new(sections)) })
    }

    /// A row for each of its lines, as a diff, as wide as its longest. Nothing in them is found, since what's asked
    /// isn't in the transcript.
    pub(crate) fn rows(&self, cx: &App) -> Vec<AnyElement> {
        let (language, none) = (language(&self.path), Marks::default());
        self.sections.iter().flat_map(|section| numbered::rows(language, section, &none, None, cx)).collect()
    }
}

/// A file's lines in a block, under its path.
fn shown_file(toggle: &Toggle, path: &str, sections: &[Section], cx: &App) -> AnyElement {
    block::frame(cx)
        .child(block::found_label(toggle, Text::Label, path, cx))
        .child(numbered::lines(toggle, Text::File, language(path), sections, cx))
        .into_any_element()
}

/// What [`shown_file`] shows, under what went wrong, for finding.
fn shown_lines(path: &str, sections: &[Section], answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    line::shown_failure(answer, false, key, gather);
    gather.inside(key, Fold::Output, |gather| {
        gather.plain(key, Text::Label, path);
        gather.plain(key, Text::File, &numbered::shown_text(sections));
    });
}

fn chip(path: &str) -> Part {
    Part::Chip(file_name(path).to_string().into())
}

/// A new file, all of it added, numbered from the top.
fn whole(content: &str) -> Section {
    (1..).zip(content.lines()).map(|(number, text)| NumberedLine::new(Mark::Added, Some(number), text)).collect()
}

fn preview(language: &str, sections: Rc<Vec<Section>>) -> Preview {
    let language: SharedString = language.to_string().into();
    Rc::new(move |_, cx: &mut App| {
        let (language, sections) = (language.clone(), sections.clone());
        cx.new(|_| DiffPreview { language, sections: marks::marked(&sections) }).into()
    })
}

/// An edit's diff, shown while its line or the diff itself is hovered.
struct DiffPreview {
    language: SharedString,
    sections: Rc<Vec<Section>>,
}

impl Render for DiffPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // It isn't in the transcript, so it has nothing to find in.
        let none = Marks::default();
        let rows = self.sections.iter().flat_map(|section| numbered::rows(&self.language, section, &none, None, cx));
        let lines = div().id("edit-preview-lines").children(rows.collect::<Vec<_>>()).test_support();
        let preview = div()
            .region(Region::EditPreview)
            .w(px(PREVIEW_WIDTH))
            .max_h(px(PREVIEW_HEIGHT))
            .py_1()
            // The wheel scrolls the diff and not the transcript under it, which would carry the line away from it.
            .occlude()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.input)
            .bg(theme.popover)
            .font_family(theme.mono_font_family.clone())
            .text_size(theme.mono_font_size)
            // It has nothing to hand the wheel on to, so GPUI scrolls it, holding a gesture to the way it started until
            // it turns far enough. That's GPUI's own, which the kit's `lock_scroll_axis` calls, since the kit gives that
            // only to an element that isn't also marked for tests, as a region is.
            .overflow_scroll()
            .restrict_scroll_to_axis();
        holding_wide(preview, lines)
    }
}
