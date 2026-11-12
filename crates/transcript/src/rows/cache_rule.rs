//! Where a prompt found the cache expired: a rule across the chat, drawn as a compaction's is, after the prompt and
//! before what the agent wrote, saying how long the chat had been idle and how much its request wrote again. Only in a
//! chat big enough for that to cost something, as the rest of what's said of the cache is.

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, Role, StatefulInteractiveElement as _, Styled, TestSupportExt as _,
};
use look::CacheMark;
use protocol::model::{CACHE_WORTH_TELLING_TOKENS, Seq};

use super::{empty, row, rule_across, rule_middle};
use crate::item_key::ItemKey;

const MINUTE_MS: u64 = 60_000;

/// The rule for the prompt's request at `seq`, which read `read_tokens`, what every chat shares, and wrote
/// `written_tokens` again, `idle_ms` after the request before it. Nothing in a chat whose request held less than
/// [`CACHE_WORTH_TELLING_TOKENS`] in all.
pub(super) fn line(seq: Seq, idle_ms: u64, read_tokens: u64, written_tokens: u64, cx: &App) -> AnyElement {
    if read_tokens.saturating_add(written_tokens) < CACHE_WORTH_TELLING_TOKENS {
        return empty();
    }
    let said = |between: &str| {
        let parts = ["Cache expired".to_string(), format!("{} idle", idle(idle_ms)), written(written_tokens)];
        parts.join(between)
    };
    let middle = rule_middle(ItemKey::Event(seq, 0).element("cache-expired"))
        .role(Role::Label)
        .aria_label(said(", "))
        .child(look::cache_mark(CacheMark::Clock, look::faint()))
        .child(said(" · "))
        .test_support();
    row().pt_2().child(rule_across(middle, cx)).into_any_element()
}

/// How long the chat was idle, in its two largest units: "12m", "1h 20m", "2d 3h".
fn idle(idle_ms: u64) -> String {
    let minutes = idle_ms / MINUTE_MS;
    let (days, hours, minutes) = (minutes / (24 * 60), minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, hours) => format!("{hours}h {minutes}m"),
        (days, hours) => format!("{days}d {hours}h"),
    }
}

fn written(tokens: u64) -> String {
    format!("{} tokens written again", look::tokens(tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_the_chat_was_idle_is_in_its_two_largest_units() {
        assert_eq!(idle(12 * MINUTE_MS + 59_999), "12m");
        assert_eq!(idle(80 * MINUTE_MS), "1h 20m");
        assert_eq!(idle((2 * 24 * 60 + 3 * 60 + 5) * MINUTE_MS), "2d 3h");
    }

    #[test]
    fn what_was_written_again_is_counted_as_the_cards_count_it() {
        assert_eq!(written(313_500), "313.5k tokens written again");
    }
}
