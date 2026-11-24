use client_core::records::{HubRecords, Snapshot};
use protocol::fixtures::observed;
use protocol::model::{
    CacheTtl, ContextUsage, ConversationCache, Observation, PromptCache, Records, Seq, SessionStatus,
};

use super::*;

const MINUTE_MS: i64 = 60_000;
const SENT_MS: i64 = 1_793_541_600_000;

/// A chat with a turn behind it.
fn chat(id: &str) -> Session {
    Session { last_seq: Seq::new(4), ..protocol::fixtures::session(id) }
}

/// What the chat `id`'s agent said of its context, which holds `used_tokens`, and what the API's cache holds of the
/// chat, as the agent's newest request left it, sent at `sent_ms` with a 1h cache.
fn cache(id: &str, used_tokens: u64, sent_ms: i64) -> Vec<Observation> {
    let context =
        ContextUsage { used_tokens, max_tokens: 1_000_000, model_tokens: None, max_set_by: None, parts: Vec::new() };
    let held = PromptCache { used_at_ms: sent_ms, ttl: CacheTtl::OneHour, model: None, tokens: 313_500 };
    let cache = ConversationCache::worked_out(Some(held), Some(&context));
    vec![observed::context(id, context), observed::cache(id, cache)]
}

/// A client's copy of a hub's records that holds `chats`, and what the API's cache holds of each, as `caches` say.
fn holding<'a>(chats: impl IntoIterator<Item = &'a Session>, caches: &[Vec<Observation>]) -> Snapshot {
    let mut held = HubRecords::default();
    let sessions = chats.into_iter().cloned().collect();
    held.apply_list(Records { sessions, observations: caches.concat(), ..Records::default() });
    held.snapshot()
}

fn after(minutes: i64) -> i64 {
    SENT_MS + minutes * MINUTE_MS
}

#[test]
fn a_big_chat_says_its_cache_expires_soon_only_once_its_about_to() {
    let big = chat("a");
    let records = holding([&big], &[cache("a", 313_500, SENT_MS)]);

    assert_eq!(expiring_in_ms(&records, &big, after(44)), None, "warm");
    assert_eq!(expiring_in_ms(&records, &big, after(51)), Some(9 * MINUTE_MS));
    assert_eq!(look::expires_in(9 * MINUTE_MS), "Cache expires in 9 min");
    assert_eq!(expiring_in_ms(&records, &big, after(60)), None, "expired, which the tab and the row leave unsaid");
}

#[test]
fn a_small_chat_or_one_whose_agent_works_says_nothing_of_its_cache() {
    let small = chat("a");
    let working = Session { status: SessionStatus::Running, ..chat("b") };
    let closed = Session { closed_at_ms: Some(1), ..chat("c") };
    let caches = [cache("a", 9_000, SENT_MS), cache("b", 313_500, SENT_MS), cache("c", 313_500, SENT_MS)];
    let records = holding([&small, &working, &closed], &caches);

    assert_eq!(expiring_in_ms(&records, &small, after(51)), None);
    assert_eq!(expiring_in_ms(&records, &working, after(51)), None);
    assert_eq!(expired_line(&records, &small, after(72)), None);
    assert_eq!(expiring_in_ms(&records, &closed, after(51)), None, "a closed chat");
}

#[test]
fn the_soonest_to_expire_of_a_workspaces_chats_is_the_one_its_row_says() {
    let (later, sooner) = (chat("b"), chat("a"));
    let records = holding([&later, &sooner], &[cache("b", 313_500, after(5)), cache("a", 313_500, SENT_MS)]);

    assert_eq!(soonest_expiring(&records, [&later, &sooner], after(51)), Some(9 * MINUTE_MS));
}

#[test]
fn once_it_has_expired_the_composer_says_when_and_what_the_next_prompt_writes_again() {
    let big = chat("a");
    let records = holding([&big], &[cache("a", 313_500, SENT_MS)]);

    assert_eq!(expired_line(&records, &big, after(59)), None);
    assert_eq!(
        expired_line(&records, &big, after(72)).as_deref(),
        Some("Cache expired 12m ago · this prompt writes 313k tokens again")
    );
}

#[test]
fn the_picker_says_what_switching_costs_only_while_the_cache_is_warm() {
    let big = chat("a");
    let records = holding([&big], &[cache("a", 313_500, SENT_MS)]);

    assert_eq!(
        switching_note(&records, &big, after(10)).as_deref(),
        Some("Switching writes this chat's 313k tokens to the cache again")
    );
    assert!(switching_note(&records, &big, after(51)).is_some(), "about to expire, but still warm");
    assert_eq!(switching_note(&records, &big, after(61)), None);
}

#[test]
fn the_window_is_drawn_again_when_what_is_said_changes_and_not_before() {
    let (big, small) = (chat("a"), chat("c"));
    let records = holding([&big, &small], &[cache("a", 313_500, SENT_MS), cache("c", 9_000, SENT_MS)]);
    let next = |now_ms| next_change_in_ms(&records, [&big], now_ms);

    assert_eq!(next(after(40)), Some(5 * MINUTE_MS), "warm, until it's about to expire");
    assert_eq!(next(after(50) + 1_000), Some(MINUTE_MS - 1_000), "about to expire, as each minute goes");
    assert_eq!(next(after(61)), None, "expired, which the tabs, the sidebar and Home leave unsaid");
    assert_eq!(next_change_in_ms(&records, [&small], after(40)), None, "a small chat says nothing");
}

#[test]
fn it_waits_no_longer_than_a_few_minutes_since_a_sleeping_mac_stops_its_timers() {
    let big = chat("a");
    let records = holding([&big], &[cache("a", 313_500, SENT_MS)]);

    assert_eq!(next_change_in_ms(&records, [&big], after(10)), Some(LONGEST_WAIT_MS));
}

#[test]
fn the_line_above_the_composer_changes_once_the_cache_expires_and_as_how_long_ago_it_did_changes() {
    let (big, small) = (chat("a"), chat("c"));
    let records = holding([&big, &small], &[cache("a", 313_500, SENT_MS), cache("c", 9_000, SENT_MS)]);
    let next = |now_ms| composer_line_changes_in_ms(&records, &big, now_ms);

    assert_eq!(next(after(58)), Some(2 * MINUTE_MS), "until it shows");
    assert_eq!(next(after(61) + 1_000), Some(MINUTE_MS - 1_000), "as each minute goes");
    assert_eq!(next(after(60 + 125)), Some(LONGEST_WAIT_MS), "then by the hour, looked at every few minutes");
    assert_eq!(composer_line_changes_in_ms(&records, &small, after(61)), None, "a small chat shows no line");
}
