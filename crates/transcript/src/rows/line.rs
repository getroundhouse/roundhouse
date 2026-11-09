//! The one line every tool call is drawn as: an icon for its kind, what it did, and the details that matter.
//! Clicking it opens and closes what's under it. Each kind of call fills in the parts, and the line is the same for
//! all of them.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, px, relative,
};

use super::tools::{Toggle, block};
use crate::calls::lines::Counts;
use crate::calls::{Failure, failure};
use crate::rows::around::Answer;

/// How far what a line opens to sits in from it, so it lines up under the title.
pub(super) const INDENT: f32 = 24.0;
const ICON: f32 = 15.0;
/// As tall as a line of body text and a little room, so calls in a row read as a list.
pub(super) const HEIGHT: f32 = 28.0;
/// The hover group of one line, which turns its icon into the way to open it.
const GROUP: &str = "tool-line";

pub(super) struct Line {
    icon: IconName,
    title: SharedString,
    parts: Vec<Part>,
    preview: Option<Preview>,
    /// Whether what the call opens to shows what came back, so a failure's whole text isn't added under its line.
    shows_output: bool,
}

/// What shows while a closed line is hovered, on a device that has hover, such as a change's diff.
pub(super) type Preview = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// A detail after the title.
pub(super) enum Part {
    /// Code, such as a command, on a raised pill.
    Pill(SharedString),
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
        Self { icon, title: title.into(), parts: Vec::new(), preview: None, shows_output: false }
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

    pub(super) fn part(mut self, part: impl Into<Option<Part>>) -> Self {
        self.parts.extend(part.into());
        self
    }

    pub(super) fn render(self, toggle: &Toggle, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (this, key, seq, open) = (toggle.this.clone(), toggle.key.clone(), toggle.seq, toggle.open);
        let hover = theme.muted;
        let parts = self.parts.into_iter().map(|part| part.render(cx));
        h_flex()
            .id(toggle.key.element("call"))
            .group(GROUP)
            .role(Role::Button)
            .aria_label(self.title.clone())
            .aria_expanded(open)
            .h(px(HEIGHT))
            .mx(px(-8.))
            .px(px(8.))
            .gap_2()
            .rounded(px(6.))
            .overflow_hidden()
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |d| d.bg(hover))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), !open, cx));
            })
            .when_some(self.preview.filter(|_| !open), |d, preview| d.tooltip(move |window, cx| preview(window, cx)))
            .child(icon(self.icon, open, cx))
            .child(div().flex_none().max_w(relative(0.7)).overflow_hidden().text_ellipsis().child(self.title))
            .children(parts)
            .test_support()
            .into_any_element()
    }
}

impl Part {
    fn render(self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        match self {
            Part::Pill(text) => pill(text, theme.muted_foreground, theme.secondary, cx),
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
            Part::Code(text) => div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .font_family(theme.mono_font_family.clone())
                .text_size(theme.mono_font_size)
                .text_color(look::faint())
                .child(text)
                .into_any_element(),
            Part::Note(text) => div().flex_none().text_color(look::faint()).child(text).into_any_element(),
            Part::Summary(text) => div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(look::faint())
                .child(text)
                .into_any_element(),
            Part::Counts(Counts { added, removed }) => h_flex()
                .flex_none()
                .gap_1()
                .font_family(theme.mono_font_family.clone())
                .text_size(theme.mono_font_size)
                .child(div().text_color(theme.success).child(format!("+{added}")))
                .child(div().text_color(theme.danger).child(format!("−{removed}")))
                .into_any_element(),
        }
    }
}

/// The kind's icon, which turns into the way to open or close the line while it's hovered.
fn icon(kind: IconName, open: bool, cx: &App) -> Div {
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    let layer = || div().absolute().top_0().left_0();
    div()
        .relative()
        .flex_none()
        .size(px(ICON))
        .child(
            layer()
                .group_hover(GROUP, Styled::invisible)
                .child(Icon::new(kind).size(px(ICON)).text_color(look::faint())),
        )
        .child(
            layer()
                .invisible()
                .group_hover(GROUP, Styled::visible)
                .child(Icon::new(chevron).size(px(ICON)).text_color(cx.theme().foreground)),
        )
}

/// Code on a pill, cut off with an ellipsis when it doesn't fit.
fn pill(text: SharedString, color: Hsla, background: Hsla, cx: &App) -> AnyElement {
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
        .child(text)
        .into_any_element()
}

/// A call's line, then a line saying it failed if its result says so, then what it opens to while it's open.
pub(super) fn drawn(
    line: Line,
    answer: Option<&Answer>,
    opened: Option<AnyElement>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let failed = answer.and_then(Answer::failure);
    // The line under a failed call has room for the start of what went wrong, so opened, the call shows all of it.
    let whole = failed.map(failure::output).filter(|text| toggle.open && !line.shows_output && !text.trim().is_empty());
    v_flex()
        .child(line.render(toggle, cx))
        .when_some(failed, |d, text| d.child(failure(&Failure::read(text), cx)))
        .when_some(whole, |d, text| {
            let output = block::output(toggle.key.element("error"), text, cx.theme().muted_foreground, cx);
            d.child(block::frame(cx).child(output))
        })
        .children(opened)
        .into_any_element()
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
            d.child(pill(failure.message.to_string().into(), danger, danger.opacity(0.09), cx))
        })
        .into_any_element()
}
