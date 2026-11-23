//! The one line every tool call is drawn as: an icon for its kind, what it did, and the details that matter.
//! Clicking it opens and closes what's under it. Each kind of call fills in the parts, and the line is the same for
//! all of them. A line with nothing under it, such as a task's that a turn the agent started by itself took in, is
//! drawn the same, but doesn't open.

use std::rc::Rc;

use client_core::calls::{Failure, first_line};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, relative,
};
use look::{Hand as _, IconSizing as _, LineText, RightClickMenu as _, Shaped as _, Tooltipped as _};
use protocol::event::Denied;

use super::shown::Gather;
use super::tools::{Toggle, block};
use crate::calls::lines::Counts;
use crate::find::{self, Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::around::{Answer, Outcome};

/// How big a call's icon is, at the start of its line, and the chevron that takes its place.
pub(super) const ICON: look::IconSize = look::IconSize::Large;
/// A small row's, so calls in a row read as a list.
pub(super) const SIZE: look::RowSize = look::RowSize::Small;
/// The hover group of one line, which turns its icon into the way to open it.
const GROUP: &str = "tool-line";
/// More of prose's start than a line has room for, so a long first line isn't copied to be cut off.
pub(super) const START_CHARS: usize = 160;

pub(super) struct Line {
    icon: IconName,
    title: SharedString,
    /// Where the matches are in the title, when it's the agent's words.
    title_marks: Marks,
    parts: Vec<Part>,
    preview: Option<Preview>,
    /// Whether what the call opens to shows what came back, so a failure's whole text isn't added under its line.
    shows_output: bool,
    /// A button at the line's far end, which always shows, so the way it offers is seen before the pointer finds it.
    action: Option<AnyElement>,
    /// What a long press on the line copies under touch, when it isn't the title: what the call acted on, such as its
    /// command or its file.
    copies: Option<SharedString>,
}

/// What shows while a closed line is hovered, on a device that has hover, such as a change's diff. It stays while the
/// pointer is on it, so what it holds can be scrolled.
pub(super) type Preview = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// A detail after the title.
pub(super) enum Part {
    /// Code, such as a command, on a raised pill, with the matches in it, if it's found in.
    Pill(SharedString, Marks),
    /// What it acted on, which the app knows, such as a file or an MCP server, on a chip.
    Chip(look::Chip),
    /// What's true of it, in a word or two, such as a question's header, on a badge.
    Badge(look::Badge),
    /// An input, in faint monospace.
    Code(SharedString),
    /// A quiet word or two, such as the tool's name.
    Note(SharedString),
    /// A line of prose, faint, cut off with an ellipsis where the line ends, such as the start of what the agent
    /// thought.
    Summary(SharedString),
    /// How many lines a change added and removed.
    Counts(Counts),
}

impl Line {
    pub(super) fn new(icon: IconName, title: impl Into<SharedString>) -> Self {
        Self {
            icon,
            title: title.into(),
            title_marks: Marks::default(),
            parts: Vec::new(),
            preview: None,
            shows_output: false,
            action: None,
            copies: None,
        }
    }

    /// A line whose title is the agent's words, such as what it said a call is for, which can be found in.
    pub(super) fn found(mut self, toggle: &Toggle) -> Self {
        self.title_marks = toggle.marks(Text::Title, &self.title);
        self
    }

    /// Says whether what the call opens to shows what came back, failures included.
    pub(super) fn shows_output(mut self, shows: bool) -> Self {
        self.shows_output = shows;
        self
    }

    pub(super) fn preview(mut self, preview: Preview) -> Self {
        self.preview = Some(preview);
        self
    }

    pub(super) fn action(mut self, action: AnyElement) -> Self {
        self.action = Some(action);
        self
    }

    /// Has the menu a long press on the line opens under touch copy `text`, what the call acted on, such as its
    /// command, a file's path, or what it searched for, rather than the line's title, which every other line copies.
    pub(super) fn copies(mut self, text: impl Into<SharedString>) -> Self {
        self.copies = Some(text.into());
        self
    }

    pub(super) fn part(mut self, part: impl Into<Option<Part>>) -> Self {
        self.parts.extend(part.into());
        self
    }

    pub(super) fn render(mut self, toggle: &Toggle, cx: &App) -> AnyElement {
        let (this, key, seq, fold, open) =
            (toggle.shared.this.clone(), toggle.key.clone(), toggle.seq, toggle.fold, toggle.open);
        let set_open = move |cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), fold, !open, cx));
        };
        // Under touch there's no preview, and a tap opens the line to what it would show.
        let preview = self.preview.take().filter(|_| !open);
        let frame = opening(toggle.key.element("call"), self.title.clone(), open, set_open, cx)
            .when_some(preview, |d, preview| d.previews(move |window, cx| preview(window, cx), cx));
        let title = find::marked(self.title.clone(), &self.title_marks, &[], None);
        let lead = lead(glyph(self.icon, cx), look::Size::Inline.width(), open, cx);
        let copies = self.copies.take().unwrap_or_else(|| self.title.clone());
        self.fill(frame, lead, title, &toggle.shared.line_text, cx).test_support().copied_by_a_long_press(copies, cx)
    }

    /// The line with nothing under it to open, named `id`, and `label` to assistive apps, such as a task a turn the
    /// agent started by itself took in: the same as a call's, but with no way to open it, and nothing lit under the
    /// pointer.
    pub(super) fn still(mut self, id: ElementId, label: String, line_text: &LineText, cx: &App) -> AnyElement {
        let frame = frame().id(id).role(Role::Label).aria_label(label);
        let (lead, title) = (still_icon(self.icon, cx), self.title.clone());
        let copies = self.copies.take().unwrap_or_else(|| self.title.clone());
        self.fill(frame, lead, title, line_text, cx).test_support().copied_by_a_long_press(copies, cx)
    }

    /// `frame`, holding what every line does after `lead`: the title, which takes at most most of the line, then the
    /// parts, then a button at the far end. The title and the parts give way to the button rather than pushing it off,
    /// since it always shows: the parts that are cut off where they end first, such as a summary, and then the title,
    /// which still takes at most most of what's left.
    fn fill(
        self,
        frame: Stateful<Div>,
        lead: impl IntoElement,
        title: impl IntoElement,
        line_text: &LineText,
        cx: &App,
    ) -> Stateful<Div> {
        let title = div().flex_none().max_w(relative(0.7)).overflow_hidden().text_ellipsis().child(title);
        let title = line_text.body.set(title);
        let parts = self.parts.into_iter().map(|part| part.render(line_text, cx));
        let frame = frame.child(lead);
        match self.action {
            None => frame.child(title).children(parts),
            Some(action) => frame
                .child(
                    h_flex().flex_1().min_w_0().overflow_hidden().gap(look::Space::Small).child(title).children(parts),
                )
                .child(div().flex_none().child(action)),
        }
    }
}

/// What a line is drawn in: as tall as every line, its parts side by side, cut off where it ends.
fn frame() -> Div {
    h_flex().h(SIZE.height()).gap(look::Space::Small).overflow_hidden().whitespace_nowrap()
}

impl Part {
    /// The start of some prose, such as a thought, as a summary.
    pub(super) fn start_of(text: &str) -> Self {
        Part::Summary(start(text).into())
    }

    /// Code, a note, a summary and counts go on the title's baseline. A pill and a chip are shapes, so they're centred.
    fn render(self, line_text: &LineText, cx: &App) -> AnyElement {
        let colors = look::colors(cx);
        match self {
            Part::Pill(text, marks) => {
                look::pill(look::PillKind::Plain, find::marked(text, &marks, &[], None)).into_any_element()
            }
            Part::Chip(chip) => chip.into_any_element(),
            Part::Badge(badge) => badge.into_any_element(),
            Part::Code(text) => line_text
                .code
                .set(div().min_w_0().overflow_hidden().text_ellipsis().text_color(colors.text_faint).child(text))
                .into_any_element(),
            Part::Note(text) => {
                line_text.body.set(div().flex_none().text_color(colors.text_faint).child(text)).into_any_element()
            }
            Part::Summary(text) => line_text
                .body
                .set(div().min_w_0().overflow_hidden().text_ellipsis().text_color(colors.text_faint).child(text))
                .into_any_element(),
            Part::Counts(Counts { added, removed }) => {
                look::lines_changed(look::LinesKind::Plain, added, removed).placed(&line_text.code).into_any_element()
            }
        }
    }
}

/// A line that opens and closes what's under it when it's clicked, before what it holds: as tall as every line, lit
/// under the pointer, and a button to assistive apps, named `label`, which says whether it's open. A tool call's line
/// is one, and so is the line for several background tasks at the chat's end.
pub(super) fn opening(
    id: ElementId,
    label: SharedString,
    open: bool,
    on_click: impl Fn(&mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let hover = look::colors(cx).line_hover;
    frame()
        .id(id)
        .group(GROUP)
        .role(Role::Button)
        .aria_label(label)
        .aria_expanded(open)
        .mx(-look::Space::Small.size())
        .px(look::Space::Small)
        .corners(look::Corner::Control)
        .hand()
        .hover(move |d| d.bg(hover))
        .on_click(move |_, _, cx| on_click(cx))
}

/// What a line that opens starts with, such as its kind's icon, `width` wide, which turns into the way to open or
/// close the line while it's hovered.
pub(super) fn lead(mark: impl IntoElement, width: Pixels, open: bool, cx: &App) -> Div {
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    let layer = || div().absolute().top_0().left_0();
    div().relative().flex_none().size(width).child(layer().group_hover(GROUP, Styled::invisible).child(mark)).child(
        layer()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .invisible()
            .group_hover(GROUP, Styled::visible)
            .child(Icon::new(chevron).sized(ICON).text_color(look::colors(cx).text)),
    )
}

/// The kind's icon as a line starts with it, for a line that doesn't open, such as one that's still streaming. Its
/// place is as wide as a mark beside text, as the working line's and a task's are, so their words line up.
pub(super) fn still_icon(kind: IconName, cx: &App) -> Div {
    div().flex_none().size(look::Size::Inline.width()).child(glyph(kind, cx))
}

fn glyph(kind: IconName, cx: &App) -> Icon {
    Icon::new(kind).sized(ICON).text_color(look::colors(cx).text_faint)
}

/// A call's line, then a line saying it failed if its result says so, or that the person denied it, then what `opened`
/// makes of what it opens to, which it's asked only while the call's open. That's made after what went wrong, as it's
/// shown after it, since a call nested in another's row has its texts placed in the order they're made.
pub(super) fn drawn(
    line: Line,
    answer: Option<&Answer>,
    opened: impl FnOnce() -> Option<AnyElement>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let failed = answer.and_then(Answer::failure);
    // The line under a failed call has room for the start of what went wrong, so opened, the call shows all of it.
    let whole =
        failed.map(|failed| failed.output).filter(|text| toggle.open && !line.shows_output && !text.trim().is_empty());
    let whole = whole.map(|text| {
        let output = block::output(toggle, Text::Error, text, look::colors(cx).text_muted, cx);
        block::frame(cx).child(output)
    });
    let opened = if toggle.open { opened() } else { None };
    v_flex()
        .child(line.render(toggle, cx))
        .when_some(failed, |d, failed| d.child(failure(&failed, cx)))
        .when_some(answer.and_then(denied), |d, denied| d.child(denial(denied, toggle, cx)))
        .children(whole)
        .children(opened)
        .into_any_element()
}

/// What [`drawn`] shows open of what went wrong, for finding: all of it, under the line, when what the call opens
/// to doesn't show it already.
pub(super) fn shown_failure(answer: Option<&Answer>, shows_output: bool, key: &ItemKey, gather: &mut Gather) {
    let failed = answer.and_then(Answer::failure).map(|failed| failed.output).filter(|text| !text.trim().is_empty());
    if let Some(text) = failed.filter(|_| !shows_output) {
        gather.inside(key, Fold::Output, |gather| gather.plain(key, Text::Error, &block::output_text(text)));
    }
}

/// The line under a call whose result says it failed, so it shows while the call is closed.
fn failure(failure: &Failure, cx: &App) -> AnyElement {
    let error_colour = look::colors(cx).error;
    frame()
        .child(Icon::new(IconName::CircleX).flex_none().sized(ICON).text_color(error_colour))
        .child(div().flex_none().text_color(error_colour).child(failure.label()))
        .when(!failure.message.is_empty(), |d| {
            d.child(look::pill(look::PillKind::Failure, failure.message.to_string()))
        })
        .into_any_element()
}

fn denied(answer: &Answer) -> Option<&Denied> {
    if let Outcome::Denied(denied) = answer.outcome() { Some(denied) } else { None }
}

/// What the person said as they denied the call, on the line under it, which shows closed or open.
fn said(denied: &Denied) -> Option<&str> {
    denied.said.as_deref().map(str::trim).filter(|said| !said.is_empty()).map(first_line)
}

/// What [`drawn`] shows of a denial, for finding: what the person said, whether the call is open or not.
pub(super) fn shown_denial(answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    if let Some(said) = answer.and_then(denied).and_then(said) {
        gather.plain(key, Text::Said, said);
    }
}

/// The line under a call the person denied, quiet, since it's what they chose: `Denied`, then what they told the agent,
/// cut off where the line ends.
fn denial(denied: &Denied, toggle: &Toggle, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let said = said(denied);
    frame()
        .id("call-denied")
        .role(Role::Label)
        .aria_label(said.map_or_else(|| "Denied".to_string(), |said| format!("Denied: {said}")))
        .child(Icon::new(IconName::CircleX).flex_none().sized(ICON).text_color(colors.text_faint))
        .child(div().flex_none().text_color(muted).child("Denied"))
        .when_some(said, |d, said| {
            let words = find::marked(said.to_string().into(), &toggle.marks(Text::Said, said), &[], None);
            d.child(div().min_w_0().overflow_hidden().text_ellipsis().text_color(muted).child(words))
        })
        .test_support()
        .into_any_element()
}

/// The start of some prose, for a line: its first line with anything on it, without the marks for code and emphasis,
/// which the line can't draw.
fn start(text: &str) -> String {
    first_line(text).chars().filter(|c| !matches!(c, '`' | '*')).take(START_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::start;

    #[test]
    fn the_start_of_prose_leaves_out_markdown_s_marks() {
        assert_eq!(start("`slow` takes a **target** speed\nand more"), "slow takes a target speed");
    }

    #[test]
    fn prose_that_starts_with_a_blank_line_starts_where_its_words_do() {
        assert_eq!(start("\n  \nThe test says a train should slow."), "The test says a train should slow.");
    }
}
