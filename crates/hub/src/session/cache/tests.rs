use protocol::model::{CacheTtl, ModelName};

use super::*;

const OPUS: ModelName = ModelName::named("claude-opus-5-5");
const HAIKU: ModelName = ModelName::named("claude-haiku-5-5");

/// What a request to Opus sent at 1 s left: 252.8k tokens of a conversation, warm for an hour.
fn before() -> PromptCache {
    PromptCache { used_at_ms: 1_000, ttl: CacheTtl::OneHour, model: Some(OPUS), tokens: 252_800 }
}

/// What a request to `model` that read `read_tokens` and wrote `written_tokens`, sent at 1 s, left in the cache.
fn asked(model: ModelName, read_tokens: u64, written_tokens: u64) -> PromptCache {
    let request = CacheRequest { model: Some(model), ..request(read_tokens, written_tokens) };
    after(None, &request, 1_000).unwrap()
}

fn request(read_tokens: u64, written_tokens: u64) -> CacheRequest {
    CacheRequest {
        sent_ms_ago: 0,
        ttl: Some(CacheTtl::OneHour),
        read_tokens,
        written_tokens,
        model: Some(OPUS),
        first_for_prompt: true,
    }
}

#[test]
fn a_prompts_first_request_that_read_little_of_what_was_cached_found_the_cache_expired() {
    let found = found_expired(Some(&before()), &request(14_800, 238_000), 4_801_000);

    assert_eq!(
        found,
        Some(EventBody::CacheExpired { idle_ms: 4_800_000, read_tokens: 14_800, written_tokens: 238_000 })
    );
}

#[test]
fn one_that_read_what_was_cached_found_it_warm_however_much_the_prompt_added() {
    // Two of the CLI's real requests in a row, in `crates/agents/tests/fixtures/claude.jsonl`.
    let (warm, next) = (asked(HAIKU, 27_087, 204), CacheRequest { model: Some(HAIKU), ..request(27_291, 56) });
    let small = PromptCache { tokens: 30_000, ..before() };

    assert_eq!(found_expired(Some(&warm), &next, 2_000), None);
    assert_eq!(found_expired(Some(&small), &request(30_000, 40_000), 2_000), None, "a long paste");
}

#[test]
fn nothing_is_found_without_a_request_before_or_by_one_that_isnt_a_prompts_first() {
    let not_first = CacheRequest { first_for_prompt: false, ..request(14_800, 238_000) };
    let unsaid = PromptCache { tokens: 0, ..before() };

    assert_eq!(found_expired(None, &request(14_800, 238_000), 2_000), None);
    assert_eq!(found_expired(Some(&before()), &not_first, 2_000), None);
    assert_eq!(found_expired(Some(&unsaid), &request(14_800, 238_000), 2_000), None, "a hub that didn't keep it");
}

#[test]
fn a_request_to_another_model_isnt_one_that_found_the_cache_expired() {
    // Real requests too: the last to Haiku, then the first to Sonnet, which read only what every chat shares.
    let (haiku, sonnet) = (asked(HAIKU, 27_473, 242), ModelName::named("claude-sonnet-5-5"));
    let other = CacheRequest { model: Some(sonnet), ..request(12_932, 16_854) };
    let unsaid = CacheRequest { model: None, ..other.clone() };

    assert_eq!(found_expired(Some(&haiku), &other, 2_000), None);
    assert!(found_expired(Some(&haiku), &unsaid, 2_000).is_some(), "which only its model tells apart");
}

#[test]
fn a_request_that_wrote_nothing_keeps_the_ttl_of_what_it_read_and_one_that_wrote_takes_its_own() {
    let haiku = HAIKU;
    let read_only = CacheRequest { ttl: None, model: None, ..request(250_000, 0) };
    let wrote = CacheRequest { ttl: Some(CacheTtl::FiveMinutes), model: Some(haiku.clone()), ..request(0, 9_000) };

    assert_eq!(after(Some(&before()), &read_only, 9), Some(PromptCache { used_at_ms: 9, tokens: 250_000, ..before() }));
    assert_eq!(
        after(Some(&before()), &wrote, 9),
        Some(PromptCache { used_at_ms: 9, ttl: CacheTtl::FiveMinutes, model: Some(haiku), tokens: 9_000 })
    );
    assert_eq!(after(None, &read_only, 9), None);
}
