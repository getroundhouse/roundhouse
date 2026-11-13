//! The top of the panel: the question's header, a small detail, then its text, at body size.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::Hand as _;
use protocol::event::Question;

use super::QuestionPanel;

/// The height of the header's line, at the kit's extra-small size.
const HEADER_LINE: f32 = 16.;

/// The height of a line of the question, at body size.
const QUESTION_LINE: f32 = 20.;

/// Between the parts of the header.
const BETWEEN: &str = " · ";

/// The question's header, then what the subagent that asks is for, when one does, then the question's text. Across
/// from the header is the button that skips what's asked, for what can be skipped.
pub(super) fn heading(
    panel: &QuestionPanel,
    question: Option<&Question>,
    cx: &mut Context<QuestionPanel>,
) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    let named = question.and_then(|question| question.header.as_deref());
    let header = named.into_iter().chain(panel.asker.as_deref()).collect::<Vec<_>>().join(BETWEEN);
    let skips = panel.asks.skipped().is_some();
    // A small control's square around an icon as big as the text's line, standing out past the line rather than
    // pushing the question down, so the icon lines up with the edges inside the panel.
    let skip = skips.then(|| {
        Button::new("question-skip")
            .hand()
            .ghost()
            .small()
            .size_6()
            .p_0()
            .my(px(-4.))
            .mr(px(-4.))
            .child(Icon::new(IconName::Close).size_4().text_color(muted))
            .tooltip("Skip")
            .on_click(cx.listener(|this, _, _, cx| this.skip(cx)))
    });
    let text = question.map(|question| {
        said("question-text", question.text.clone()).w_full().text_sm().line_height(px(QUESTION_LINE)).test_support()
    });
    v_flex()
        .w_full()
        .gap_1()
        .child(
            h_flex()
                .w_full()
                .h(px(HEADER_LINE))
                .gap_2()
                .child(
                    said("question-header", header)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .test_support(),
                )
                .children(skip),
        )
        .children(text)
        .into_any_element()
}

/// `text` as words the panel says, which assistive apps read as it's drawn, for the caller to style.
pub(super) fn said(id: &'static str, text: impl Into<SharedString>) -> Stateful<Div> {
    let text: SharedString = text.into();
    div().id(id).role(Role::Label).aria_label(text.clone()).child(text)
}
