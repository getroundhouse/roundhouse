//! A dot for each question, between arrows to the one before and after. A filled dot has an answer, and a ring
//! goes round the one showing. In its place, where what's asked is one question, how many more requests wait behind
//! it.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Hand as _, Off as _};
use protocol::event::QuestionAnswer;

use super::QuestionPanel;
use super::heading::said;

/// How many more requests wait to be asked once this one is answered, as a small detail, or nothing with none. It
/// starts where the rows' numbers do.
pub(super) fn more_waiting(more: usize, cx: &mut Context<QuestionPanel>) -> Option<AnyElement> {
    let muted = cx.theme().muted_foreground;
    let waiting =
        |more| said("question-more-waiting", format!("{more} more waiting")).pl_2().text_xs().text_color(muted);
    (more > 0).then(|| waiting(more).test_support().into_any_element())
}

/// Nothing when there's only one question, since there's nowhere to go.
pub(super) fn pager(
    panel: &QuestionPanel,
    answered: &[Option<QuestionAnswer>],
    cx: &mut Context<QuestionPanel>,
) -> Option<AnyElement> {
    let count = panel.asked.len();
    if count < 2 {
        return None;
    }
    let showing = panel.showing;
    let theme = cx.theme();
    let (foreground, muted) = (theme.foreground, theme.muted_foreground);
    let dots: Vec<AnyElement> = answered
        .iter()
        .enumerate()
        .map(|(q, answer)| {
            let inner = div()
                .size(px(8.))
                .rounded_full()
                .map(|d| if answer.is_some() { d.bg(foreground) } else { d.border_1().border_color(muted) });
            div()
                .id(ElementId::Name(format!("question-dot-{q}").into()))
                .size(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .hand()
                .when(q == showing, |d| d.border_1().border_color(muted))
                .on_click(cx.listener(move |this, _, window, cx| this.show(q, window, cx)))
                .child(inner)
                .test_support()
                .into_any_element()
        })
        .collect();
    let arrow =
        |id: &'static str, icon: IconName, tooltip: &'static str, by: isize, cx: &mut Context<QuestionPanel>| {
            let ends = if by < 0 { showing == 0 } else { showing + 1 == count };
            Button::new(id)
                .ghost()
                .xsmall()
                .icon(icon)
                // Only while it can be pressed, since the kit lays this over its own greyed-out colour.
                .when(!ends, |b| b.text_color(muted))
                .tooltip(tooltip)
                .off_while(ends)
                .on_click(cx.listener(move |this, _, window, cx| this.step(by, window, cx)))
        };
    let back = arrow("question-back", IconName::ChevronLeft, "Previous question", -1, cx);
    let next = arrow("question-next", IconName::ChevronRight, "Next question", 1, cx);
    Some(
        h_flex()
            .items_center()
            // The arrows' squares stand out past the panel's edge inside, so their icons line up with it.
            .ml(px(-4.))
            .child(back)
            .child(h_flex().gap_0p5().children(dots))
            .child(next)
            .into_any_element(),
    )
}
