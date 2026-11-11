//! Reading, writing and editing a file: what the call did and to which file, with how many lines where that helps.
//! Opened, the file's lines with their numbers, and a change as a diff. Hovering an edit shows its diff, which scrolls
//! both ways.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, px,
};

use super::line::{Line, Part, Preview};
use super::{Answer, Toggle, amount, block, line, numbered};
use crate::calls::diff;
use crate::calls::files::{file_name, language, numbered_lines};
use crate::calls::lines::{Mark, NumberedLine, Section};
use crate::calls::{EditFile, ReadFile, WriteFile, count};
use crate::scroll_area::holding_wide;

const PREVIEW_WIDTH: f32 = 560.0;
/// A dozen lines and half of the next, so a preview never covers much of the transcript, and a longer diff shows that
/// it scrolls.
const PREVIEW_HEIGHT: f32 = 260.0;

pub(super) fn read(file: &ReadFile, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let lines = answer.and_then(Answer::success).map(numbered_lines).unwrap_or_default();
    let images = answer.map(Answer::images).unwrap_or_default();
    let title = amount(lines.len(), images.len()).map_or_else(|| "Read".to_string(), |amount| format!("Read {amount}"));
    let line = Line::new(IconName::FileText, title).part(chip(file.path));
    // What came back that isn't the file's lines, if it isn't blank. A read with nothing to show opens to nothing, as
    // one of an image recorded before the hub kept a tool's images does. A failure is shown under the line.
    let text = answer.and_then(Answer::success).filter(|text| lines.is_empty() && !text.trim().is_empty());
    let output = |text| block::output(toggle.key.element("output"), text, cx.theme().muted_foreground, cx);
    let opened = if !toggle.open {
        None
    } else if lines.is_empty() && images.is_empty() {
        text.map(|text| block::frame(cx).child(output(text)).into_any_element())
    } else {
        // Its lines, or what else came back, then any images, such as a notebook's.
        let section: Section = lines.iter().map(|(number, text)| NumberedLine::same(*number, text)).collect();
        let shown = block::frame(cx)
            .child(block::label(file.path.to_string(), cx))
            .when(!lines.is_empty(), |d| {
                d.child(numbered::lines(toggle.key.element("file"), language(file.path), &[section], cx))
            })
            .when_some(text, |d, text| d.child(output(text)))
            .when(!images.is_empty(), |d| d.child(block::images(images, cx)));
        Some(shown.into_any_element())
    };
    line::drawn(line, answer, opened, toggle, cx)
}

pub(super) fn write(file: &WriteFile, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    // A write over a file that was there is a change, counted like an edit. Otherwise all of it is new.
    let replaced = answer.and_then(Answer::hunks);
    let icon = if replaced.is_some() { IconName::FilePen } else { IconName::FilePlus };
    let title = format!("Write {}", count(file.content.lines().count(), "line", "lines"));
    let line = Line::new(icon, title)
        .part(chip(file.path))
        .part(replaced.map(|hunks| Part::Counts(diff::change_counts(hunks))));
    let opened = toggle.open.then(|| {
        let sections = replaced.map_or_else(|| vec![whole(file.content)], diff::from_hunks);
        shown_file(toggle, file.path, &sections, cx)
    });
    line::drawn(line, answer, opened, toggle, cx)
}

pub(super) fn edit(file: &EditFile, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let sections: Vec<Section> = answer
        .and_then(Answer::hunks)
        .map_or_else(|| file.edits.iter().map(|(old, new)| diff::from_text(old, new)).collect(), diff::from_hunks);
    let sections = Rc::new(sections);
    let line = Line::new(IconName::FilePen, "Edit")
        .part(chip(file.path))
        .part(Part::Counts(diff::counts(&sections)))
        .preview(preview(language(file.path), sections.clone()));
    let opened = toggle.open.then(|| shown_file(toggle, file.path, &sections, cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// A file's lines in a block, under its path.
fn shown_file(toggle: &Toggle, path: &str, sections: &[Section], cx: &App) -> AnyElement {
    block::frame(cx)
        .child(block::label(path.to_string(), cx))
        .child(numbered::lines(toggle.key.element("file"), language(path), sections, cx))
        .into_any_element()
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
        cx.new(|_| DiffPreview { language, sections }).into()
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
        let rows = self.sections.iter().flat_map(|section| numbered::rows(&self.language, section, cx));
        let lines = div().id("edit-preview-lines").children(rows.collect::<Vec<_>>()).test_support();
        let preview = div()
            .id("edit-preview")
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
            // it turns far enough.
            .overflow_scroll()
            .lock_scroll_axis();
        holding_wide(preview, lines).test_support()
    }
}
