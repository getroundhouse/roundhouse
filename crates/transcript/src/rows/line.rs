//! The one line every tool call is drawn as: an icon for its kind, what it did, and the details that matter.
//! Clicking it opens and closes what's under it. Each kind of call fills in the parts, and the line is the same for
//! all of them.

use std::rc::Rc;

use client_core::calls::{Failure, first_line};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, Div, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, px, relative,
};
use look::{Hand as _, LineText};
use protocol::event::Denied;

use super::shown::Gather;
use super::tools::{Toggle, block};
use crate::calls::lines::Counts;
use crate::find::{self, Marks, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::around::{Answer, Outcome};

const ICON: f32 = look::LINE_ICON;
/// As tall as a line of body text and a little room, so calls in a row read as a list.
pub(super) const HEIGHT: f32 = 28.0;
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
}

/// What shows while a closed line is hovered, on a device that has hover, such as a change's diff. It stays while the
/// pointer is on it, so what it holds can be scrolled.
pub(super) type Preview = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// A detail after the title.
pub(super) enum Part {
    /// Code, such as a command, on a raised pill, with the matches in it, if it's found in.
    Pill(SharedString, Marks),
    /// What it acted on, such as an MCP server, in an outlined chip.
    Chip(SharedString),
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

    pub(super) fn part(mut self, part: impl Into<Option<Part>>) -> Self {
        self.parts.extend(part.into());
        self
    }

    pub(super) fn render(self, toggle: &Toggle, cx: &App) -> AnyElement {
        let (this, key, seq, fold, open) =
            (toggle.shared.this.clone(), toggle.key.clone(), toggle.seq, toggle.fold, toggle.open);
        let line_text = &toggle.shared.line_text;
        let parts = self.parts.into_iter().map(|part| part.render(line_text, cx));
        let set_open = move |cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), fold, !open, cx));
        };
        opening(toggle.key.element("call"), self.title.clone(), open, set_open, cx)
            .when_some(self.preview.filter(|_| !open), |d, preview| {
                d.hoverable_tooltip(move |window, cx| preview(window, cx))
            })
            .child(lead(glyph(self.icon), px(ICON), open, cx))
            .child(
                line_text.body.set(
                    div()
                        .flex_none()
                        .max_w(relative(0.7))
                        .overflow_hidden()
                        .text_ellipsis()
                        // A button always shows at the end, so the title gives way to it rather than pushing it off.
                        .when(self.action.is_some(), |d| d.flex_shrink(1.).min_w_0())
                        .child(find::marked(self.title, &self.title_marks, &[], None)),
                ),
            )
            .children(parts)
            .when_some(self.action, |d, action| d.child(div().flex_1()).child(div().flex_none().child(action)))
            .test_support()
            .into_any_element()
    }
}

impl Part {
    /// The start of some prose, such as a thought, as a summary.
    pub(super) fn start_of(text: &str) -> Self {
        Part::Summary(start(text).into())
    }

    /// Code, a note, a summary and counts go on the title's baseline. A pill and a chip are shapes, so they're centred.
    fn render(self, line_text: &LineText, cx: &App) -> AnyElement {
        let theme = cx.theme();
        match self {
            Part::Pill(text, marks) => pill(text, &marks, theme.muted_foreground, theme.secondary, cx),
            Part::Chip(text) => div()
                .flex_none()
                .px_2()
                .rounded(px(6.))
                .border_1()
                .border_color(theme.input)
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
                .into_any_element(),
            Part::Code(text) => line_text
                .code
                .set(div().min_w_0().overflow_hidden().text_ellipsis().text_color(look::faint()).child(text))
                .into_any_element(),
            Part::Note(text) => {
                line_text.body.set(div().flex_none().text_color(look::faint()).child(text)).into_any_element()
            }
            Part::Summary(text) => line_text
                .body
                .set(div().min_w_0().overflow_hidden().text_ellipsis().text_color(look::faint()).child(text))
                .into_any_element(),
            Part::Counts(Counts { added, removed }) => line_text
                .code
                .set(
                    h_flex()
                        .flex_none()
                        .gap_1()
                        .child(div().text_color(theme.success).child(format!("+{added}")))
                        .child(div().text_color(theme.danger).child(format!("−{removed}"))),
                )
                .into_any_element(),
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
    let hover = cx.theme().muted;
    h_flex()
        .id(id)
        .group(GROUP)
        .role(Role::Button)
        .aria_label(label)
        .aria_expanded(open)
        .h(px(HEIGHT))
        .mx(px(-8.))
        .px(px(8.))
        .gap_2()
        .rounded(px(6.))
        .overflow_hidden()
        .whitespace_nowrap()
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
            .child(Icon::new(chevron).size(px(ICON)).text_color(cx.theme().foreground)),
    )
}

/// The kind's icon as a line starts with it, for a line that doesn't open, such as one that's still streaming.
pub(super) fn still_icon(kind: IconName) -> Div {
    div().flex_none().size(px(ICON)).child(glyph(kind))
}

fn glyph(kind: IconName) -> Icon {
    Icon::new(kind).size(px(ICON)).text_color(look::faint())
}

/// Code on a pill, cut off with an ellipsis when it doesn't fit, with its matches.
fn pill(text: SharedString, marks: &Marks, color: Hsla, background: Hsla, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .min_w_0()
        .px_2()
        .py(px(1.))
        .rounded(px(6.))
        .bg(background)
        .overflow_hidden()
        .text_ellipsis()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(color)
        .child(find::marked(text, marks, &[], None))
        .into_any_element()
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
        let output = block::output(toggle, Text::Error, text, cx.theme().muted_foreground, cx);
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
    let danger = cx.theme().danger;
    h_flex()
        .h(px(HEIGHT))
        .gap_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(Icon::new(IconName::CircleX).flex_none().size(px(ICON)).text_color(danger))
        .child(div().flex_none().text_color(danger).child(failure.label()))
        .when(!failure.message.is_empty(), |d| {
            d.child(pill(failure.message.to_string().into(), &Marks::default(), danger, danger.opacity(0.09), cx))
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
    let muted = cx.theme().muted_foreground;
    let said = said(denied);
    h_flex()
        .id("call-denied")
        .h(px(HEIGHT))
        .gap_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .role(Role::Label)
        .aria_label(said.map_or_else(|| "Denied".to_string(), |said| format!("Denied: {said}")))
        .child(Icon::new(IconName::CircleX).flex_none().size(px(ICON)).text_color(look::faint()))
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
