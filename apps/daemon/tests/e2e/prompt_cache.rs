//! What the API's cache holds of a chat's conversation, on its chat's record, and a prompt that found it expired, in
//! the log.

#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use client_core::Connection;
use protocol::capability::settings::Settings;
use protocol::event::EventBody;
use protocol::model::{CacheTtl, ModelName, Observation, PromptCache, SessionId};

use crate::support::{self, Stack, wait_for_turn_end, wait_until};

fn now_ms() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

/// What the hub's level says the cache holds of the chat's conversation, once that's `as_expected`. The hub tells of the
/// cache as a turn ends, a moment after the chat's record says the turn has, so what's read between the two is the
/// cache as it was before the turn.
fn cache_once(
    client: &Connection,
    session: &SessionId,
    what: &str,
    as_expected: impl Fn(Option<&PromptCache>) -> bool,
) -> Option<PromptCache> {
    let mut cache = None;
    wait_until(what, || {
        cache = support::at_the_hub(client, |observation| match observation {
            Observation::SessionCache { session_id, cache, .. } if session_id == session => {
                support::known(cache).and_then(|cache| cache.held.clone())
            }
            _ => None,
        });
        as_expected(cache.as_ref())
    });
    cache
}

/// What the cache holds of the chat's conversation, once the hub says it holds some.
pub(crate) fn cache_held(client: &Connection, session: &SessionId) -> PromptCache {
    cache_once(client, session, "the cache to hold the conversation", |cache| cache.is_some()).unwrap()
}

#[test]
fn the_hub_says_when_a_chats_agents_newest_request_was_sent_and_for_how_long_its_cache_lasts() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let nothing_cached = |cache: Option<&PromptCache>| cache.is_none();
    cache_once(&client, &session.id, "the agent hasn't asked anything yet", nothing_cached);

    let before = now_ms();
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let after = now_ms();

    let cache = cache_held(&client, &session.id);
    assert_eq!((cache.ttl, cache.model.clone()), (CacheTtl::OneHour, Some(ModelName::named("fake-model"))));
    // Sent as long before the hub heard of it as the model took to start answering, a minute in the fake.
    assert!((before - 60_000..=after - 60_000).contains(&cache.used_at_ms), "{before} {cache:?} {after}");
}

#[test]
fn a_prompt_whose_request_wrote_the_conversation_again_found_the_cache_expired_and_one_that_read_it_didnt() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let PromptCache { used_at_ms: first_sent, .. } = cache_held(&client, &session.id);

    client.session_prompt(&session.id, "cold", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let prompt = events.iter().rfind(|e| matches!(e.body, EventBody::UserPrompt { .. })).unwrap().seq;
    let cold = cache_once(&client, &session.id, "the cold prompt's request", |cache| {
        cache.is_some_and(|cache| cache.used_at_ms != first_sent)
    });
    let cold_sent = cold.unwrap().used_at_ms;
    client.session_prompt(&session.id, "say:warm again", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 3);

    let expired: Vec<_> = events.iter().filter(|e| matches!(e.body, EventBody::CacheExpired { .. })).collect();
    assert_eq!(expired.len(), 1, "only the cold prompt's request found it expired: {expired:?}");
    let EventBody::CacheExpired { idle_ms, read_tokens, written_tokens } = expired[0].body else { unreachable!() };
    assert_eq!((read_tokens, written_tokens), (14_800, 238_000));
    assert!(expired[0].seq > prompt, "after the prompt it found it, before what the agent wrote");
    // Each turn of the fake makes one request, so the idle time is from the first turn's to the second's.
    assert_eq!(idle_ms, u64::try_from(cold_sent - first_sent).unwrap());
}

#[test]
fn a_compacted_chats_cache_holds_none_of_its_conversation_and_a_later_request_fills_it_again() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    cache_held(&client, &session.id);

    client.session_compact(&session.id, None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);
    cache_once(&client, &session.id, "the cache to hold none of the conversation", |cache| cache.is_none());

    client.session_prompt(&session.id, "say:hello again", None).unwrap();
    wait_for_turn_end(&client, &session.id, 3);
    cache_held(&client, &session.id);
}

#[test]
fn choosing_another_model_leaves_nothing_cached_and_choosing_the_same_leaves_the_cache() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    cache_held(&client, &session.id);
    let model = |model| Settings { model: Some(model), ..Settings::default() };

    client.session_configure(&session.id, model(session.model.clone())).unwrap();
    cache_held(&client, &session.id);
    client.session_configure(&session.id, model(ModelName::named("haiku"))).unwrap();
    // The next request is to a model whose cache doesn't hold the conversation.
    cache_once(&client, &session.id, "nothing cached for the other model", |cache| cache.is_none());
}
