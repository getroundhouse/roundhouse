use protocol::model::{CacheTtl, ContextUsage, PromptCache, SessionStatus};

use super::*;

const MINUTE_MS: i64 = 60_000;
const SENT_MS: i64 = 1_793_541_600_000;

/// A chat whose agent's newest request was sent at [`SENT_MS`], with a 1h cache and `used_tokens` in its context.
fn chat(id: &str, used_tokens: u64) -> Session {
    let context =
        ContextUsage { used_tokens, max_tokens: 1_000_000, model_tokens: None, max_set_by: None, parts: Vec::new() };
    Session {
        last_seq: 4,
        context: Some(context),
        prompt_cache: Some(PromptCache { used_at_ms: SENT_MS, ttl: CacheTtl::OneHour, model: None, tokens: 313_500 }),
        ..protocol::fixtures::session(id)
    }
}

fn after(minutes: i64) -> i64 {
    SENT_MS + minutes * MINUTE_MS
}

#[test]
fn a_big_chat_says_its_cache_expires_soon_only_once_its_about_to() {
    let big = chat("a", 313_500);

    assert_eq!(expiring_in_ms(&big, after(44)), None, "warm");
    assert_eq!(expiring_in_ms(&big, after(51)), Some(9 * MINUTE_MS));
    assert_eq!(look::expires_in(9 * MINUTE_MS), "Cache expires in 9 min");
    assert_eq!(expiring_in_ms(&big, after(60)), None, "expired, which the tab and the row leave unsaid");
}

#[test]
fn a_small_chat_or_one_whose_agent_works_says_nothing_of_its_cache() {
    let mut working = chat("a", 313_500);
    working.status = SessionStatus::Running;

    assert_eq!(expiring_in_ms(&chat("a", 9_000), after(51)), None);
    assert_eq!(expiring_in_ms(&working, after(51)), None);
    assert_eq!(expired_line(&chat("a", 9_000), after(72)), None);
    let closed = Session { closed_at_ms: Some(1), ..chat("a", 313_500) };
    assert_eq!(expiring_in_ms(&closed, after(51)), None, "a closed chat");
}

#[test]
fn the_soonest_to_expire_of_a_workspaces_chats_is_the_one_its_row_says() {
    let mut later = chat("b", 313_500);
    later.prompt_cache.as_mut().unwrap().used_at_ms = after(5);

    assert_eq!(soonest_expiring([&later, &chat("a", 313_500)], after(51)), Some(9 * MINUTE_MS));
}

#[test]
fn once_it_has_expired_the_composer_says_when_and_what_the_next_prompt_writes_again() {
    let big = chat("a", 313_500);

    assert_eq!(expired_line(&big, after(59)), None);
    assert_eq!(
        expired_line(&big, after(72)).as_deref(),
        Some("Cache expired 12m ago · this prompt writes 313.5k tokens again")
    );
}

#[test]
fn the_picker_says_what_switching_costs_only_while_the_cache_is_warm() {
    let big = chat("a", 313_500);

    assert_eq!(
        switching_note(&big, after(10)).as_deref(),
        Some("Switching writes this chat's 313.5k tokens to the cache again")
    );
    assert!(switching_note(&big, after(51)).is_some(), "about to expire, but still warm");
    assert_eq!(switching_note(&big, after(61)), None);
}

#[test]
fn the_window_is_drawn_again_when_what_is_said_changes_and_not_before() {
    let big = chat("a", 313_500);
    let next = |now_ms| next_change_in_ms([&big], now_ms);

    assert_eq!(next(after(40)), Some(5 * MINUTE_MS), "warm, until it's about to expire");
    assert_eq!(next(after(50) + 1_000), Some(MINUTE_MS - 1_000), "about to expire, as each minute goes");
    assert_eq!(next(after(61)), None, "expired, which the tabs, the sidebar and Home leave unsaid");
    assert_eq!(next_change_in_ms([&chat("c", 9_000)], after(40)), None, "a small chat says nothing");
}

#[test]
fn it_waits_no_longer_than_a_few_minutes_since_a_sleeping_mac_stops_its_timers() {
    assert_eq!(next_change_in_ms([&chat("a", 313_500)], after(10)), Some(LONGEST_WAIT_MS));
}

#[test]
fn the_line_above_the_composer_changes_once_the_cache_expires_and_as_how_long_ago_it_did_changes() {
    let big = chat("a", 313_500);
    let next = |now_ms| composer_line_changes_in_ms(&big, now_ms);

    assert_eq!(next(after(58)), Some(2 * MINUTE_MS), "until it shows");
    assert_eq!(next(after(61) + 1_000), Some(MINUTE_MS - 1_000), "as each minute goes");
    assert_eq!(next(after(60 + 125)), Some(LONGEST_WAIT_MS), "then by the hour, looked at every few minutes");
    assert_eq!(composer_line_changes_in_ms(&chat("c", 9_000), after(61)), None, "a small chat shows no line");
}
