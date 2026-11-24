//! How full an agent's context is, on its chat's record.

#![cfg(test)]

use client_core::{Client, SubscriptionItem};
use protocol::client::HubNotice;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::event::EventBody;
use protocol::model::{ContextPart, ContextPartKind, ContextUsage, MaxSetBy, Observation, Observed, Seq, SessionId};

use crate::accounts::wait_for_adas_week;
use crate::support::{Observing, Stack, TIMEOUT, known, wait_for_turn_end};

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
    let said = |observation: &Observation| match observation {
        Observation::SessionContext { session_id, context, .. } if session_id == session => known(context).cloned(),
        _ => None,
    };
    let mut chat = Observing::at(client, Follow::Session { id: session.clone(), from: None });
    chat.until("the chat's level to say how full the context is", said, |said| said == Some(context));
}

#[test]
fn a_chats_level_says_how_full_its_agents_context_is_and_follows_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let level = Follow::Session { id: session.id.clone(), from: None };
    let followed = client.follow(level).unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    // The session's level says so, as it lists itself and then as it changes, so a client that follows the chat sees
    // it. What the turn's start said may come first, which can be from before anything said how much the model holds.
    let said = |observation: &Observation| match observation {
        Observation::SessionContext { context: Observed::AsOf { value, .. }, .. } => Some(value.clone()),
        _ => None,
    };
    let mut told = std::iter::from_fn(|| followed.items.recv_timeout(TIMEOUT).ok()).flat_map(|item| match item {
        SubscriptionItem::Listed(listed) => match *listed {
            Listed::Session(records) => records.observations.iter().filter_map(said).collect(),
            _ => Vec::new(),
        },
        SubscriptionItem::Notice(notice) => match *notice {
            HubNotice::Observation(observation) => said(&observation).into_iter().collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    });
    assert_eq!(told.find(|context| context == &after_prompts(1)), Some(after_prompts(1)));

    client.session_prompt(&session.id, "say:more", None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(told.find(|context| context == &after_prompts(2)), Some(after_prompts(2)));
}

#[test]
fn how_full_the_context_is_and_the_accounts_limits_stay_out_of_the_log_and_the_limits_reach_the_account() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));
    // What the agent heard of its account's limits is the account's, which a client reads on its record.
    wait_for_adas_week(&client, &session.id);

    // The record says how full the context is, and the log has nothing of it: it answers the worker's own question,
    // and the answer also lists the user's skills by path.
    let (events, _) = client.session_read(&session.id, Seq::new(1), 500, true).unwrap();
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("get_context_usage")));
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("skillFrontmatter")));
    // Nor of the limits, which the agent says in a report, outside the log, though its CLI said them in a line.
    assert!(events.iter().all(|e| !matches!(e.body, EventBody::AgentLimits { .. })), "{events:?}");
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("rate_limit_event")));
}
