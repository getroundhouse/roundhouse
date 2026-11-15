//! The question's options, one numbered row each, and the row under them for typing another answer. A chosen row is
//! lit, and where several can be chosen, it has a check.

use gpui_kit::component::input::{Escape, Input, Textarea};
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Hand as _, IconSizing as _, Shaped as _, Typeset as _};
use protocol::event::{Question, QuestionOption};

use super::QuestionPanel;

/// About seven options with a line of description each. A longer list scrolls, so the transcript keeps its room.
const OPTIONS_MOST_HEIGHT: f32 = 360.;

/// The colours a row is drawn in.
#[derive(Clone, Copy)]
struct Colors {
    foreground: Hsla,
    muted: Hsla,
    chosen: Hsla,
    hovered: Hsla,
}

pub(super) fn list(
    panel: &QuestionPanel,
    question: &Question,
    other: &super::typing::Field,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    let app = look::colors(cx);
    let colors = Colors {
        foreground: app.text,
        muted: app.text_muted,
        // The kit keeps its own colour for a list's chosen row faint, so this takes a raised surface's hover, the
        // nearest opaque step above the pane.
        chosen: app.raised_hover,
        hovered: app.row_hover,
    };
    let choice = panel.asked.get(panel.showing).map_or_default(|asked| asked.choice.clone());
    let rows: Vec<AnyElement> = question
        .options
        .iter()
        .enumerate()
        .map(|(o, option)| option_row(o, option, choice.has(o), question.multiple, colors, cx))
        .collect();
    v_flex()
        .id("question-options")
        .w_full()
        .max_h(px(OPTIONS_MOST_HEIGHT))
        .overflow_y_scroll()
        .gap(look::Space::XSmall)
        .children(rows)
        .when(panel.asks.can_type(panel.showing), |d| {
            d.child(other_row(other, choice.other(), question.multiple, colors, cx))
        })
        .into_any_element()
}

/// The height of a row's first line, at body size, which its number and check are centred on.
const LINE: f32 = 20.;

/// The height of a line of description, at the kit's extra-small size.
const DESCRIPTION_LINE: f32 = 16.;

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
        .h(px(LINE))
        .flex()
        .items_center()
        .typeset(look::Type::Detail)
        .text_color(muted)
        .child(n.to_string())
}

fn row(id: ElementId, chosen: bool, colors: Colors) -> gpui_kit::Stateful<gpui_kit::Div> {
    h_flex()
        .id(id)
        .w_full()
        .items_start()
        .gap(look::Space::Medium)
        .p(look::Space::Small)
        .corners(look::Corner::Control)
        .typeset(look::Type::Body)
        .line_height(px(LINE))
        .hand()
        .map(|d| if chosen { d.bg(colors.chosen) } else { d.hover(move |style| style.bg(colors.hovered)) })
}

fn check(muted: Hsla) -> impl IntoElement {
    let check = Icon::new(IconName::Check).sized(look::IconSize::Regular).text_color(muted);
    // As wide as the checkbox of the row to type in, so the two are in line down the rows' ends.
    div().flex_none().w(px(CHECKBOX)).h(px(LINE)).flex().items_center().justify_center().child(check)
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
            .line_height(px(DESCRIPTION_LINE))
            .text_color(colors.muted)
            .child(description)
    });
    row(ElementId::Name(format!("question-answer-{o}").into()), chosen, colors)
        .on_click(cx.listener(move |this, _, window, cx| this.pick(o, window, cx)))
        .child(number(o + 1, colors.muted))
        .child(v_flex().flex_1().min_w_0().child(option.label.clone()).children(description))
        .when(multiple && chosen, |d| d.child(check(colors.muted)))
        .test_support()
        .into_any_element()
}

/// Clicking anywhere on it but its checkbox goes to its text, and Escape comes back out, so the number keys choose again.
fn other_row(
    other: &super::typing::Field,
    chosen: bool,
    multiple: bool,
    colors: Colors,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    // Its own target, so it can take back what's typed without going to the text, which would choose it again.
    let checkbox = div()
        .id("question-other-check")
        .flex_none()
        .mt(px((LINE - CHECKBOX) / 2.))
        .size(px(CHECKBOX))
        .flex()
        .items_center()
        .justify_center()
        .corners(look::Corner::InlineControl)
        .border_1()
        .border_color(colors.foreground)
        .hand()
        .on_click(cx.listener(|this, _, window, cx| {
            cx.stop_propagation();
            this.toggle_typed(window, cx);
        }))
        .when(chosen, |d| d.child(Icon::new(IconName::Check).sized(look::IconSize::Small)))
        .test_support();
    row("question-other".into(), chosen, colors)
        .on_click(cx.listener(|this, _, window, cx| this.type_other(window, cx)))
        .on_action(cx.listener(|this, _: &Escape, window, cx| {
            let showing = this.showing;
            this.show(showing, window, cx);
        }))
        .child(number(0, colors.muted))
        // Its smallest size has the least padding, which leaves its text where the options' labels start.
        .child(div().flex_1().min_w_0().ml(px(-TYPING_INSET)).child(match other {
            super::typing::Field::Text(input) => {
                Textarea::new(input).appearance(false).xsmall().typeset(look::Type::Body).into_any_element()
            }
            super::typing::Field::Secret(input) => {
                Input::new(input).appearance(false).xsmall().typeset(look::Type::Body).into_any_element()
            }
        }))
        .when(multiple, |d| d.child(checkbox))
        .test_support()
        .into_any_element()
}
