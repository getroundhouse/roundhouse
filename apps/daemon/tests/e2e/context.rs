//! How full an agent's context is, on its chat's record.

#![cfg(test)]

use client_core::{Client, SubscriptionItem};
use protocol::client::HubNotice;
use protocol::event::EventBody;
use protocol::model::{ContextPart, ContextPartKind, ContextUsage, MaxSetBy, SessionId};

use crate::accounts::the_week;
use crate::support::{Stack, TIMEOUT, wait_for_turn_end, wait_until};

/// What the fake agent says its context holds once a conversation has seen this many prompts, and a turn has ended
/// since it launched. Its settings make the context a fifth of what its model holds, which only a turn's end says.
pub(crate) fn after_prompts(prompts: u64) -> ContextUsage {
    let part = |name: &str, tokens, kind| ContextPart { name: name.to_string(), tokens, kind };
    let messages = prompts * 1_000;
    ContextUsage {
        used_tokens: 15_000 + messages,
        max_tokens: 200_000,
        model_tokens: Some(1_000_000),
        max_set_by: Some(MaxSetBy::Settings),
        parts: vec![
            part("System prompt", 3_000, ContextPartKind::Used),
            part("System tools", 12_000, ContextPartKind::Used),
            part("System tools (deferred)", 19_000, ContextPartKind::Deferred),
            part("Messages", messages, ContextPartKind::Used),
            part("Autocompact buffer", 33_000, ContextPartKind::Buffer),
            part("Free space", 152_000 - messages, ContextPartKind::Free),
        ],
    }
}

pub(crate) fn wait_for_context(client: &Client, session: &SessionId, context: &ContextUsage) {
    wait_until("the record to say how full the context is", || {
        client.session_get(session).unwrap().context.as_ref() == Some(context)
    });
}

#[test]
fn a_chats_record_says_how_full_its_agents_context_is_and_follows_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    assert_eq!(session.context, None, "nothing has said yet");
    let feed = client.feed_subscribe().unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));

    // The feed says so, so a client that isn't asking sees it. What the turn's start said may come first, which can be
    // from before anything said how much the model holds.
    let mut told = std::iter::from_fn(|| feed.items.recv_timeout(TIMEOUT).ok()).filter_map(|item| match item {
        SubscriptionItem::Notice(notice) => match *notice {
            HubNotice::Session(told) => told.context,
            _ => None,
        },
        _ => None,
    });
    assert_eq!(told.find(|context| context == &after_prompts(1)), Some(after_prompts(1)));

    client.session_prompt(&session.id, "say:more", None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);
    wait_for_context(&client, &session.id, &after_prompts(2));
}

#[test]
fn how_full_the_context_is_stays_out_of_the_log_and_the_accounts_limits_go_in() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));

    // The record says how full the context is, and the log has nothing of it: it answers the worker's own question,
    // and the answer also lists the user's skills by path.
    let (events, _) = client.session_read(&session.id, 1, 500, true).unwrap();
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("get_context_usage")));
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("skillFrontmatter")));
    // What the agent heard of its account's limits is something it was told as it worked, so it's an event, with the
    // line it came in.
    let heard: Vec<_> = events.iter().filter(|e| matches!(e.body, EventBody::AgentLimits { .. })).collect();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0].body, EventBody::AgentLimits { limits: vec![the_week()] });
    assert!(heard[0].raw.as_deref().is_some_and(|raw| raw.contains("rate_limit_event")));
}

#[test]
fn a_chat_still_says_how_full_its_context_was_after_the_hub_restarts() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));

    stack.kill_hub();
    stack.start_hub();

    assert_eq!(stack.client().session_get(&session.id).unwrap().context, Some(after_prompts(1)));
}
