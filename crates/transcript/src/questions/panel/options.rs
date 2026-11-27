//! The question's options, one numbered row each, and the row under them for typing another answer, each a list's row
//! that wraps. A chosen row is washed as a list's chosen row is, and where several can be chosen, it has a check.

use gpui_kit::component::input::{Escape, Input, Textarea};
use gpui_kit::component::{Icon, IconName, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Hand as _, IconSizing as _, Shaped as _, Typeset as _};
use protocol::capability::asking::{Question, QuestionOption};

use super::QuestionPanel;

/// About seven options with a line of description each. A longer list scrolls, so the transcript keeps its room.
const OPTIONS_MOST_HEIGHT: f32 = 360.;

/// A row's size: its room, and the least it's as tall as.
const ROW_SIZE: look::RowSize = look::RowSize::Regular;

/// The colours a row is drawn in.
#[derive(Clone, Copy)]
struct Colors {
    foreground: Hsla,
    muted: Hsla,
}

pub(super) fn list(
    panel: &QuestionPanel,
    question: &Question,
    other: &super::typing::Field,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    let app = look::colors(cx);
    let colors = Colors { foreground: app.text, muted: app.text_muted };
    let choice = panel.asked.get(panel.showing).map_or_default(|asked| asked.choice.clone());
    let rows: Vec<AnyElement> = question
        .options
        .iter()
        .enumerate()
        .map(|(o, option)| option_row(o, option, choice.has(o), question.several(), colors, cx))
        .collect();
    v_flex()
        .id("question-options")
        .w_full()
        .max_h(px(OPTIONS_MOST_HEIGHT))
        .overflow_y_scroll()
        .gap(look::Space::XSmall)
        .children(rows)
        .when(panel.asks.can_type(panel.showing), |d| {
            d.child(other_row(other, choice.other(), question.several(), colors, panel.dictation(), cx))
        })
        .into_any_element()
}

/// How far apart a row's lines are, over the size of its text, at body size: 20px at a desk. Its number and check are
/// centred on its first.
const LINES: f32 = 1.43;

/// How far apart a description's lines are, over the size of its text, a detail: 16px at a desk.
const DESCRIPTION_LINES: f32 = 1.33;

/// How far the typing row's text sits in from its edge, at its smallest size.
const TYPING_INSET: f32 = 4.;

/// The typing row's checkbox, with a small check inside it. A chosen option's check is centred in a place as wide, so
/// the two are in line down the rows' ends.
const CHECKBOX: f32 = 16.;

/// A row's number, which is also its key, as a small detail.
fn number(n: usize, muted: Hsla) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(12.))
        .h(line())
        .flex()
        .items_center()
        .typeset(look::Type::Detail)
        .text_color(muted)
        .child(n.to_string())
}

fn row(id: impl Into<ElementId>, chosen: bool) -> look::ListRow {
    look::list_row(id, ROW_SIZE).wraps().chosen(chosen)
}

/// What a row says, beside its number, on lines as tall as its number's.
fn words() -> gpui_kit::Div {
    v_flex().flex_1().min_w_0().line_height(line())
}

/// How tall a row's line is, which its number and check are as tall as.
fn line() -> Pixels {
    look::Type::Body.lines(LINES)
}

fn check(muted: Hsla) -> impl IntoElement {
    let check = Icon::new(IconName::Check).sized(look::IconSize::Regular).text_color(muted);
    // As wide as the checkbox of the row to type in, so the two are in line down the rows' ends.
    div().flex_none().w(px(CHECKBOX)).h(line()).flex().items_center().justify_center().child(check)
}

fn option_row(
    o: usize,
    option: &QuestionOption,
    chosen: bool,
    multiple: bool,
    colors: Colors,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    // What the option means, as a small detail under it.
    let description = option.description.clone().map(|description| {
        div()
            .mt(look::Space::XSmall)
            .typeset(look::Type::Detail)
            .line_height(look::Type::Detail.lines(DESCRIPTION_LINES))
            .text_color(colors.muted)
            .child(description)
    });
    row(ElementId::Name(format!("question-answer-{o}").into()), chosen)
        .on_click(cx.listener(move |this, _, window, cx| this.pick(o, window, cx)))
        .child(number(o + 1, colors.muted))
        .child(words().child(option.label.clone()).children(description))
        .when(multiple && chosen, |d| d.child(check(colors.muted)))
        .into_any_element()
}

/// Clicking anywhere on it but its checkbox goes to its text, and Escape comes back out, so the number keys choose again.
fn other_row(
    other: &super::typing::Field,
    chosen: bool,
    multiple: bool,
    colors: Colors,
    dictation: Option<&gpui_kit::Entity<dictation_ui::Field>>,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    // Its own target, so it can take back what's typed without going to the text, which would choose it again.
    let checkbox = div()
        .id("question-other-check")
        .flex_none()
        .mt((line() - px(CHECKBOX)) / 2.)
        .size(px(CHECKBOX))
        .flex()
        .items_center()
        .justify_center()
        .corners(look::Corner::InlineControl)
        .border(look::EDGE)
        .border_color(colors.foreground)
        .hand()
        .on_click(cx.listener(|this, _, window, cx| {
            cx.stop_propagation();
            this.toggle_typed(window, cx);
        }))
        .when(chosen, |d| d.child(Icon::new(IconName::Check).sized(look::IconSize::Small)))
        .test_support();
    let row = row("question-other", chosen)
        .on_click(cx.listener(|this, _, window, cx| this.type_other(window, cx)))
        .child(number(0, colors.muted))
        // Its smallest size has the least padding, which leaves its text where the options' labels start.
        .child(
            words()
                .ml(px(-TYPING_INSET))
                .child(match other {
                    super::typing::Field::Text(input) => {
                        Textarea::new(input).appearance(false).xsmall().typeset(look::Type::Body).into_any_element()
                    }
                    super::typing::Field::Secret(input) => {
                        Input::new(input).appearance(false).xsmall().typeset(look::Type::Body).into_any_element()
                    }
                })
                .children(
                    dictation.and_then(|field| field.read(cx).live_line(cx)).map(|line| line.pl(px(TYPING_INSET))),
                ),
        )
        .when(multiple, |d| d.child(checkbox));
    // Escape and dictation's keys come up from the text to what holds the row, which the line that says it's
    // listening is drawn over.
    let holder = div()
        .relative()
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            let showing = this.showing;
            this.show(showing, window, cx);
        }))
        .child(row)
        .children(dictation.map(|field| field.read(cx).edge()));
    match dictation {
        Some(field) => dictation_ui::Field::keys(holder, field).into_any_element(),
        None => holder.into_any_element(),
    }
}
