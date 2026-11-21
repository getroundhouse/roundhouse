//! A shell command: what the agent said it's for, with the command beside it. Opened, the command and then its
//! output, in one block. The command is faded, since the call's title has said what it does and what it printed is
//! what's read, and whole, since opening the call is what shows it. It's text to select, and a button at its top right
//! copies all of it, to run elsewhere.

use client_core::calls::Command;
use gpui_kit::assets::IconName;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::Typeset as _;

use super::highlight::spans;
use super::line::{Line, Part};
use super::{Answer, Outcome, Toggle, block, line};
use crate::find::{self, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;
use crate::selection::Pieces;

/// How strongly the command is drawn, so it reads as a caption to what it printed.
const STRENGTH: f32 = 0.7;
/// What the command's area is called for hovering, which shows the button that copies it.
const GROUP: &str = "command";

/// What a command's call starts with, and any line about a command.
pub(in crate::rows) const ICON: IconName = IconName::Terminal;

pub(super) fn draw(command: &Command, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| {
        let summary = command.summary();
        Part::Pill(summary.to_string().into(), toggle.marks(Text::Summary, summary))
    });
    let line = Line::new(ICON, command.title()).part(summary).shows_output(true);
    let line = if command.described().is_some() { line.found(toggle) } else { line };
    let opened = || Some(opened(command, answer, toggle, cx).into_any_element());
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
/// doesn't look as if it's still waiting. One that went on in the background says so last, since what it prints from
/// then on goes to its task rather than here. While it runs, there's only the command.
fn opened(command: &Command, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> Div {
    // In the order they're shown, since a command nested in another call's row has its texts placed in the order
    // they're made.
    let command = command_part(command, toggle, cx);
    let in_the_background = answer.is_some_and(Answer::in_the_background);
    let printed = output(answer).and_then(|text| match text.trim() {
        "" if in_the_background => None,
        "" => Some(aside(toggle, "no-output", "No output", cx)),
        _ => Some(block::lines(toggle, Text::Output, text, cx)),
    });
    let went_on = in_the_background.then(|| aside(toggle, "background", "Went on in the background", cx));
    block::frame(cx)
        .child(command)
        .children(printed.into_iter().chain(went_on).flat_map(|part| [block::separator().into_any_element(), part]))
}

/// A faint line under a command that isn't what it printed: why it printed nothing, or where what it prints next goes.
/// It isn't selectable, so a copy never takes it for output.
fn aside(toggle: &Toggle, id: &'static str, says: &'static str, cx: &App) -> AnyElement {
    div()
        .id(toggle.key.element(id))
        .px(look::Space::Medium)
        .py(look::Space::Small)
        .typeset_code(cx)
        .text_color(look::colors(cx).text_faint)
        .child(says)
        .test_support()
        .into_any_element()
}

/// The command whole, after a faint `$`, wrapping under itself rather than under the `$`. It's text to select, and a
/// button pinned at its top right, shown while it's hovered, copies all of it. It draws the matches in it.
fn command_part(command: &Command, toggle: &Toggle, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let full: SharedString = command.command.trim().to_string().into();
    let marks = toggle.marks(Text::Command, &full);
    let id = toggle.key.element("command");
    let (drawn, layout) =
        find::marked_with_layout(full.clone(), &marks, &spans("sh", &full), Some(block::scroll_id(&id)));
    // What's selected is painted under what's faded, so it's as strong as the output's.
    let text = div()
        .id(ElementId::from((id.clone(), "text")))
        .role(Role::Label)
        .aria_label(full.clone())
        .opacity(STRENGTH)
        .child(drawn)
        .test_support();
    let part = div()
        .id(id.clone())
        .flex()
        .px(look::Space::Medium)
        .py(look::Space::Small)
        .text_color(colors.text)
        .child(div().flex_none().text_color(colors.text_faint).child("$\u{a0}"))
        .child(div().flex_1().min_w_0().child(text))
        // Room for the button, which is pinned over it, so the text never runs under the button. As wide as the
        // copy button, a small button's square.
        .child(div().flex_none().ml(look::Space::Small).w(look::ControlSize::Small.height()))
        .test_support()
        .into_any_element();
    // All of it, the `$` and the space around the command too, so a press there is on the command.
    let selectable = block::selectable(toggle, Text::Command, &id, part, full.clone(), Pieces::whole(layout));
    // Half as tall as what it printed can be, so a long command leaves room for that.
    let scrolling = block::scrolling(&toggle.shared.selection, &id, selectable).max_height(px(block::MAX_HEIGHT / 2.));
    div()
        .id(ElementId::from((id.clone(), "area")))
        .relative()
        .group(GROUP)
        .typeset_code(cx)
        .child(scrolling)
        .child(copy_button(&id, full, cx))
        .test_support()
        .into_any_element()
}

/// The button that copies the command, shown while the command is hovered. It's pinned at the command's top right,
/// over what scrolls, so it stays beside the first line's place however far a long command is scrolled.
fn copy_button(id: &ElementId, full: SharedString, cx: &App) -> impl IntoElement {
    let copy = look::copy_button(ElementId::from((id.clone(), "copy")), full, "Copy command", cx);
    div()
        .absolute()
        .top_0()
        .right_0()
        .pt(look::Space::Small)
        .pr(look::Space::Medium)
        .flex()
        .items_center()
        .invisible()
        .group_hover(GROUP, Styled::visible)
        // A line's height, so the button is centred on the command's first line.
        .child(div().w_0().child("\u{a0}"))
        .child(div().h_0().flex().items_center().child(copy))
}
