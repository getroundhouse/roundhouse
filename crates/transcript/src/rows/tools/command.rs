//! A shell command: what the agent said it's for, with the command beside it. Opened, the command and then its
//! output, in one block. The command is faded and cut off at the end of its line, since the call's title has said
//! what it does and what it printed is what's read, and clicking it shows it whole.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, StyledText, TestSupportExt as _, TextLayout, div,
};

use super::highlight::spans;
use super::line::{Line, Part};
use super::{Answer, Toggle, block, line};
use crate::calls::{Command, failure};
use crate::find::Part as Text;
use crate::find::reveal;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

/// How strongly the command is drawn, so it reads as a caption to what it printed.
const STRENGTH: f32 = 0.7;

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

/// What [`draw`] shows open, for finding: the command shown whole, so a match anywhere in it shows, and what it
/// printed.
pub(super) fn shown(command: &Command, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    if let Some(title) = command.described() {
        gather.plain(key, Text::Title, title);
    }
    gather.inside(key, Fold::Output, |gather| {
        gather.inside(key, Fold::Command, |gather| gather.plain(key, Text::Command, command.command.trim()));
        if let Some(text) = output(answer).filter(|text| !text.trim().is_empty()) {
            gather.plain(key, Text::Output, &block::output_text(text));
        }
    });
}

/// What the command printed, or what went wrong, once it's back.
fn output(answer: Option<&Answer>) -> Option<&str> {
    answer.map(|a| if a.is_error { failure::output(&a.text) } else { a.text.as_str() })
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

/// The command after a faint `$`, on one line with an ellipsis where it's cut off, or whole, wrapping under itself
/// rather than under the `$`. One of several lines shows its first while it's cut off, as the call's line does.
/// Clicking it goes from one to the other, when that would change anything. It draws the matches in what shows of it.
fn command_part(command: &Command, toggle: &Toggle, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (whole, full) = (toggle.whole, command.command.trim());
    let several_lines = full != command.summary();
    let marks = toggle.marks(Text::Command, full);
    let (text, marks): (SharedString, _) = match (whole, several_lines) {
        // Its first line starts it, as both are trimmed.
        (false, true) => (format!("{}…", command.summary()).into(), marks.within(0, command.summary().len())),
        _ => (full.to_string().into(), marks),
    };
    let styled = StyledText::new(text.clone()).with_highlights(marks.over(&spans("sh", &text), text.len()));
    // Whether it's cut off is only known once it's laid out, so a press asks then.
    let layout = styled.layout().clone();
    let id = toggle.key.element("command");
    let here = marks
        .here()
        .zip(marks.reveal())
        .map(|(range, mark)| reveal::here(layout.clone(), range.start, Some(block::scroll_id(&id)), mark.clone()));
    let pressable = move || several_lines || if whole { wraps(&layout) } else { layout.text() != text.as_ref() };
    let (this, key, seq) = (toggle.shared.this.clone(), toggle.key.clone(), toggle.seq);
    let part = div()
        .id(id.clone())
        .relative()
        .flex()
        .px_3()
        .py_2()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(theme.foreground)
        .role(Role::Button)
        .aria_label(full.to_string())
        .aria_expanded(whole)
        .on_click({
            let pressable = pressable.clone();
            move |_, _, cx| {
                if pressable() {
                    let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), Fold::Command, !whole, cx));
                }
            }
        })
        .child(div().flex_none().text_color(look::faint()).child("$\u{a0}"))
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .opacity(STRENGTH)
                .when(!whole, |d| d.line_clamp(1).text_ellipsis())
                .child(styled)
                .children(here),
        )
        .child(look::hand_once_laid_out(pressable))
        .test_support();
    block::scrolling(toggle.find, &id, part).into_any_element()
}

/// Whether any of the text's lines wraps.
fn wraps(layout: &TextLayout) -> bool {
    layout.line_layouts().iter().any(|line| !line.wrap_boundaries().is_empty())
}
