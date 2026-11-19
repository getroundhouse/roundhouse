//! The line above the composer while the chat's account is using what's reserved of its week, so the person sees the
//! chat spending what they kept for themselves, and can move it. Automatic puts no new chat there by then, but a chat
//! already on it carries on.

use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _};
use look::Region;

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
    let left = week.left_percent();
    let wrong = format!("{name} is using its reserve: {left}% of its week is left, and {reserve}% is reserved.");
    let then = format!("New chats go to other accounts. This one stays on {name} until you move it.");
    // One paragraph, so what to do wraps on from what's wrong, as the push line's does.
    let banner = look::banner(look::BannerKind::Warning).region(Region::ReserveLine).child(format!("{wrong} {then}"));
    let moving = usage.moves.as_ref().map(super::context::line_button);
    Some(moving.into_iter().fold(banner, look::Banner::answer_menu).into_any_element())
}
