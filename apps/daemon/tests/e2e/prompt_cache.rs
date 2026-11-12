//! What the API's cache holds of a chat's conversation, on its chat's record, and a prompt that found it expired, in
//! the log.

#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use protocol::event::EventBody;
use protocol::model::{CacheTtl, ModelName, PromptCache, SessionSettingsChange};

use crate::support::{Stack, wait_for_turn_end};

fn now_ms() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

#[test]
fn a_chats_record_says_when_its_agents_newest_request_was_sent_and_for_how_long_its_cache_lasts() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    assert_eq!(session.prompt_cache, None, "the agent hasn't asked anything yet");

    let before = now_ms();
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let after = now_ms();

    let cache = client.session_get(&session.id).unwrap().prompt_cache.expect("the turn's request, on the record");
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
    let PromptCache { used_at_ms: first_sent, .. } = client.session_get(&session.id).unwrap().prompt_cache.unwrap();

    client.session_prompt(&session.id, "cold", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let prompt = events.iter().rfind(|e| matches!(e.body, EventBody::UserPrompt { .. })).unwrap().seq;
    let cold_sent = client.session_get(&session.id).unwrap().prompt_cache.unwrap().used_at_ms;
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
    assert!(client.session_get(&session.id).unwrap().prompt_cache.is_some());

    client.session_compact(&session.id, None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(client.session_get(&session.id).unwrap().prompt_cache, None);

    client.session_prompt(&session.id, "say:hello again", None).unwrap();
    wait_for_turn_end(&client, &session.id, 3);
    assert!(client.session_get(&session.id).unwrap().prompt_cache.is_some());
}

#[test]
fn choosing_another_model_leaves_nothing_cached_and_choosing_the_same_leaves_the_cache() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let model = |model| SessionSettingsChange { model: Some(model), ..SessionSettingsChange::default() };

    let same = client.session_configure(&session.id, model(session.model.clone())).unwrap();
    assert!(same.prompt_cache.is_some());
    let other = client.session_configure(&session.id, model(ModelName::named("haiku"))).unwrap();
    assert_eq!(other.prompt_cache, None, "the next request is to a model whose cache doesn't hold the conversation");
}

#[test]
fn a_chat_still_says_what_its_cache_holds_after_the_hub_restarts_and_still_finds_it_expired() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let kept = client.session_get(&session.id).unwrap().prompt_cache;

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();

    assert_eq!(client.session_get(&session.id).unwrap().prompt_cache, kept);
    client.session_prompt(&session.id, "cold", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let expired = events.iter().filter(|e| matches!(e.body, EventBody::CacheExpired { .. }));
    assert_eq!(expired.count(), 1, "the hub that came back knows when the request before was sent");
}
