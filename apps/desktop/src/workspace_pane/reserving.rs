//! The line above the composer while the chat's account is using what's reserved of its week, so the person sees the
//! chat spending what they kept for themselves, and can move it. Automatic puts no new chat there by then, but a chat
//! already on it carries on.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, HighlightStyle, IntoElement as _, ParentElement as _, SharedString, Styled as _, StyledText, div,
};
use look::{InRegion as _, Region, Typeset as _};

use super::context::Usage;

/// The line, while `usage`'s account is down to what it reserves of its week.
pub(super) fn reserve_line(usage: Option<&Usage>, cx: &App) -> Option<AnyElement> {
    let usage = usage?;
    let (account, now_ms) = (&usage.account, look::now_ms(cx));
    if !account.in_reserve(now_ms) {
        return None;
    }
    let (week, reserve) = (account.week(now_ms)?, account.reserve?.percent());
    let name = crate::names::called(account);
    let colors = look::colors(cx);
    let left = week.left_percent();
    let wrong = format!("{name} is using its reserve: {left}% of its week is left, and {reserve}% is reserved.");
    let then = format!("New chats go to other accounts. This one stays on {name} until you move it.");
    let warned = (0..wrong.len(), HighlightStyle { color: Some(colors.warning), ..HighlightStyle::default() });
    let words = StyledText::new(SharedString::from(format!("{wrong} {then}"))).with_highlights([warned]);
    // Out past its own room, so its words end as far in as the line's start, as the push line's button does.
    let moving = usage.moves.as_ref().map(super::context::line_button);
    Some(
        h_flex()
            .region(Region::ReserveLine)
            .flex_none()
            .w_full()
            .px(look::Space::Medium)
            .py(look::Space::XSmall)
            .gap(look::Space::Small)
            .border_t_1()
            .border_color(colors.edge)
            .typeset(look::Type::Detail)
            .text_color(colors.text_muted)
            .child(div().flex_1().min_w_0().child(words))
            .children(moving)
            .into_any_element(),
    )
}
