//! What the newest turn's line and card say of the cache: on the line, a mark once it's about to expire or has, in a
//! chat big enough for that to cost something, and in the card, how long it has left, when it expires, and how long it
//! lasts, whatever the chat's size. `docs/spec/chats.md` has the rules.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, Div, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    div,
};
use look::{CacheMark, Typeset as _};
use protocol::model::{CacheState, CacheTtl, PromptCache};

use super::card;
use crate::transcript::IdleCache;

/// The mark the line shows after its time, once the cache is about to expire, or has, in a chat where it's worth
/// telling: a wedge for what's left, then a clock. Nothing while it's warm, so most lines say nothing of it. It's in
/// muted text's colour, a step brighter than the line, so a wedge of a quarter or less still shows, and says what it
/// shows to assistive apps.
pub(super) fn on_the_line(cache: &IdleCache, cx: &App) -> Option<impl IntoElement + use<>> {
    let now_ms = look::now_ms(cx);
    let (mark, said) = match cache.cache.state(now_ms).filter(|_| cache.worth_telling)? {
        CacheState::Warm { .. } => return None,
        CacheState::Expiring { left_ms } => {
            (CacheMark::Left(cache.cache.share_left(now_ms)?), look::expires_in(left_ms))
        }
        CacheState::Expired { .. } => (CacheMark::Clock, "Cache expired".to_string()),
    };
    // Centred on the line without making it taller, as the copy button is.
    let mark = look::cache_mark(mark, look::colors(cx).text_muted);
    Some(div().id("cache").flex().items_center().h_0().aria_label(said).child(mark))
}

/// The card's part on the cache, under a rule: `Cache`, with a wedge for the share of its time it has left and how
/// long that is, or a clock and `Expired`, and under that when it expires or did, and how long it lasts. `None` for a
/// TTL this build doesn't know.
pub(super) fn in_the_card(cache: &PromptCache, cx: &App) -> Option<Div> {
    let now_ms = look::now_ms(cx);
    let (state, expires_at_ms) = (cache.state(now_ms)?, cache.expires_at_ms()?);
    let (mark, left, when) = match state {
        CacheState::Warm { left_ms } | CacheState::Expiring { left_ms } => {
            (CacheMark::Left(cache.share_left(now_ms)?), format!("{} left", look::minutes_left(left_ms)), "Expires at")
        }
        CacheState::Expired { .. } => (CacheMark::Clock, "Expired".to_string(), "Expired at"),
    };
    let muted = look::colors(cx).text_muted;
    let when = look::day_and_time(expires_at_ms, cx).map(|at| format!("{when} {at}"));
    let left = card::row("Cache", left, Some(look::cache_mark(mark, muted).into_any_element()), cx);
    let lasts = h_flex()
        .justify_between()
        .gap_6()
        .typeset(look::Type::Detail)
        .text_color(muted)
        .children(when)
        .child(div().whitespace_nowrap().child(format!("{} cache", ttl_words(cache.ttl)?)));
    Some(v_flex().gap_1().child(card::rule(cx)).child(left).child(lasts))
}

/// How long a cache lasts, as "1h".
fn ttl_words(ttl: CacheTtl) -> Option<&'static str> {
    match ttl {
        CacheTtl::FiveMinutes => Some("5m"),
        CacheTtl::OneHour => Some("1h"),
        CacheTtl::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ttl_says_how_long_it_lasts_and_one_this_build_doesnt_know_says_nothing() {
        assert_eq!(ttl_words(CacheTtl::OneHour), Some("1h"));
        assert_eq!(ttl_words(CacheTtl::FiveMinutes), Some("5m"));
        assert_eq!(ttl_words(CacheTtl::Unknown), None);
    }
}
