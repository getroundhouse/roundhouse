//! A shell command: what the agent said it's for, with the command beside it. Opened, the command and then its
//! output, in one block. The command is faded, since the call's title has said what it does and what it printed is
//! what's read, and whole, since opening the call is what shows it. It's text to select, and a button at its top right
//! copies all of it, to run elsewhere.

use client_core::calls::Command;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement as _, Styled, StyledText, TestSupportExt as _, div, px,
};

use super::highlight::spans;
use super::line::{Line, Part};
use super::{Answer, Outcome, Toggle, block, line};
use crate::find::Part as Text;
use crate::find::reveal;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;
use crate::selectable_layout::SelectableLayout;

/// How strongly the command is drawn, so it reads as a caption to what it printed.
const STRENGTH: f32 = 0.7;
/// What the command's area is called for hovering, which shows the button that copies it.
const GROUP: &str = "command";

pub(super) fn draw(command: &Command, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| {
        let summary = command.summary();
        Part::Pill(summary.to_string().into(), toggle.marks(Text::Summary, summary))
    });
    let line = Line::new(IconName::Terminal, command.title()).part(summary).shows_output(true);
    let line = if command.described().is_some() { line.found(toggle) } else { line };
    let opened = toggle.open.then(|| opened(command, answer, toggle, cx).into_any_element());
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding: the command, and what it printed.
pub(super) fn shown(command: &Command, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    if let Some(title) = command.described() {
        gather.plain(key, Text::Title, title);
    }
    gather.inside(key, Fold::Output, |gather| {
        gather.plain(key, Text::Command, command.command.trim());
        if let Some(text) = output(answer).filter(|text| !text.trim().is_empty()) {
            gather.plain(key, Text::Output, &block::output_text(text));
        }
    });
}

/// What the command printed, or what went wrong, once it's back. One the person denied never ran, so it printed
/// nothing, and its line says it was denied.
fn output(answer: Option<&Answer>) -> Option<&str> {
    match answer?.outcome() {
        Outcome::Ran(text) => Some(text),
        Outcome::Failed(failure) => Some(failure.output),
        Outcome::Denied(_) => None,
    }
}

/// The command, and under it what it printed, or a faint line saying it printed nothing, so a command that's done
/// doesn't look as if it's still waiting. While it runs, there's only the command.
fn opened(command: &Command, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> Div {
    let printed = output(answer).map(|text| match text.trim() {
        "" => nothing_printed(toggle, cx),
        _ => block::lines(toggle, Text::Output, text, cx),
    });
    block::frame(cx)
        .child(command_part(command, toggle, cx))
        .when_some(printed, |d, printed| d.child(block::separator(cx)).child(printed))
}

/// Where a command's output goes, saying there wasn't any. It isn't selectable, so a copy never takes it for output.
fn nothing_printed(toggle: &Toggle, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(toggle.key.element("no-output"))
        .px_3()
        .py_2()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(look::faint())
        .child("No output")
        .test_support()
        .into_any_element()
}

/// The command whole, after a faint `$`, wrapping under itself rather than under the `$`. It's text to select, and a
/// button pinned at its top right, shown while it's hovered, copies all of it. It draws the matches in it.
fn command_part(command: &Command, toggle: &Toggle, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let full: SharedString = command.command.trim().to_string().into();
    let marks = toggle.marks(Text::Command, &full);
    let styled = StyledText::new(full.clone()).with_highlights(marks.over(&spans("sh", &full), full.len()));
    let layout = styled.layout().clone();
    let id = toggle.key.element("command");
    let here = marks
        .here()
        .zip(marks.reveal())
        .map(|(range, mark)| reveal::here(layout.clone(), range.start, Some(block::scroll_id(&id)), mark.clone()));
    let text_id = ElementId::from((id.clone(), "text"));
    let text = div()
        .id(text_id.clone())
        .relative()
        .role(Role::Label)
        .aria_label(full.clone())
        // First, so it's painted under the text, and outside what's faded, so it's as strong as the output's.
        .child(SelectableLayout::new(text_id, full.clone(), layout))
        .child(div().opacity(STRENGTH).child(styled).children(here))
        .test_support();
    let part = div()
        .id(id.clone())
        .flex()
        .px_3()
        .py_2()
        .text_color(theme.foreground)
        .child(div().flex_none().text_color(look::faint()).child("$\u{a0}"))
        .child(div().flex_1().min_w_0().child(text))
        // Room for the button, which is pinned over it, so the text never runs under the button. As wide as the
        // kit's copy button, an extra-small icon button, which is `size_5`.
        .child(div().flex_none().ml_2().w_5())
        .test_support();
    // Half as tall as what it printed can be, so a long command leaves room for that.
    let scrolling = block::scrolling(toggle.find, &id, part).max_height(px(block::MAX_HEIGHT / 2.));
    div()
        .id(ElementId::from((id.clone(), "area")))
        .relative()
        .group(GROUP)
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .child(scrolling)
        .child(copy_button(&id, full))
        .test_support()
        .into_any_element()
}

/// The button that copies the command, shown while the command is hovered. It's pinned at the command's top right,
/// over what scrolls, so it stays beside the first line's place however far a long command is scrolled.
fn copy_button(id: &ElementId, full: SharedString) -> impl IntoElement {
    let copy = look::copy_button(ElementId::from((id.clone(), "copy")), full, "Copy command");
    div()
        .absolute()
        .top_0()
        .right_0()
        .pt_2()
        .pr_3()
        .flex()
        .items_center()
        .invisible()
        .group_hover(GROUP, Styled::visible)
        // A line's height, so the button is centred on the command's first line.
        .child(div().w_0().child("\u{a0}"))
        .child(div().h_0().flex().items_center().child(copy))
}
