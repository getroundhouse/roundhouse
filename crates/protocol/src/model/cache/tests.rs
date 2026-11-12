use super::*;

const MINUTE: i64 = 60 * 1000;

fn cache(ttl: CacheTtl) -> PromptCache {
    PromptCache { used_at_ms: 1_000_000, ttl, model: None, tokens: 0 }
}

fn at(cache: &PromptCache, minutes: i64) -> Option<CacheState> {
    cache.state(cache.used_at_ms + minutes * MINUTE)
}

#[test]
fn an_hour_long_cache_is_about_to_expire_with_a_quarter_of_its_time_left() {
    let cache = cache(CacheTtl::OneHour);

    assert_eq!(at(&cache, 0), Some(CacheState::Warm { left_ms: 60 * MINUTE }));
    assert_eq!(at(&cache, 44), Some(CacheState::Warm { left_ms: 16 * MINUTE }));
    assert_eq!(at(&cache, 45), Some(CacheState::Expiring { left_ms: 15 * MINUTE }));
    assert_eq!(at(&cache, 60), Some(CacheState::Expired { ago_ms: 0 }));
    assert_eq!(at(&cache, 72), Some(CacheState::Expired { ago_ms: 12 * MINUTE }));
    assert_eq!(at(&cache, -5), Some(CacheState::Warm { left_ms: 60 * MINUTE }), "a hub whose clock runs ahead");
    assert_eq!(cache.expires_at_ms(), Some(cache.used_at_ms + 60 * MINUTE));
    assert_eq!(cache.warns_at_ms(), Some(cache.used_at_ms + 45 * MINUTE));
}

#[test]
fn a_five_minute_cache_is_about_to_expire_with_two_minutes_left_rather_than_a_quarter() {
    let cache = cache(CacheTtl::FiveMinutes);

    assert_eq!(at(&cache, 2), Some(CacheState::Warm { left_ms: 3 * MINUTE }));
    assert_eq!(at(&cache, 3), Some(CacheState::Expiring { left_ms: 2 * MINUTE }));
    assert_eq!(at(&cache, 5), Some(CacheState::Expired { ago_ms: 0 }));
}

#[test]
fn a_ttl_this_build_doesnt_know_says_nothing_of_when_it_expires() {
    let read: CacheTtl = serde_json::from_str(r#""one_day""#).unwrap();
    let cache = cache(read);

    assert_eq!(read, CacheTtl::Unknown);
    assert_eq!(at(&cache, 1), None);
    assert_eq!(cache.expires_at_ms(), None);
}

#[test]
fn a_request_from_a_peer_that_says_less_still_reads() {
    let request: CacheRequest = serde_json::from_str("{}").unwrap();

    assert_eq!((request.ttl, request.written_tokens, request.first_for_prompt), (None, 0, false));
}

#[test]
fn the_share_of_its_time_left_goes_from_whole_to_none() {
    let cache = cache(CacheTtl::OneHour);

    assert_eq!(cache.share_left(cache.used_at_ms), Some(1.));
    assert_eq!(cache.share_left(cache.used_at_ms + 45 * MINUTE), Some(0.25));
    assert_eq!(cache.share_left(cache.used_at_ms + 90 * MINUTE), Some(0.));
    assert_eq!(cache.share_left(cache.used_at_ms - MINUTE), Some(1.), "a hub whose clock runs ahead");
}
