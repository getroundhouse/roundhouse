//! How full an agent's context is, on its chat's record, and whose account it runs under.

#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use client_core::{Client, ClientError, SubscriptionItem};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::EventBody;
use protocol::model::{
    AgentAccount, AgentKind, ContextPart, ContextPartKind, ContextUsage, LimitWindow, SessionId, UsageLimit,
};
use serde_json::json;

use crate::support::{Stack, TIMEOUT, wait_for_turn_end, wait_until};

/// What the fake agent says its context holds once a conversation has seen this many prompts.
fn after_prompts(prompts: u64) -> ContextUsage {
    let part = |name: &str, tokens, kind| ContextPart { name: name.to_string(), tokens, kind };
    let messages = prompts * 1_000;
    ContextUsage {
        used_tokens: 15_000 + messages,
        max_tokens: 200_000,
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

/// The week's limit the fake agent hears of in every turn.
fn the_week() -> UsageLimit {
    UsageLimit {
        window: LimitWindow::SevenDays,
        used_percent: 4,
        resets_at_ms: Some(4_102_444_800_000),
        heard_at_ms: None,
    }
}

fn wait_for_context(client: &Client, session: &SessionId, context: &ContextUsage) {
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

    // The feed says so, so a client that isn't asking sees it.
    let told = std::iter::from_fn(|| feed.items.recv_timeout(TIMEOUT).ok()).find_map(|item| match item {
        SubscriptionItem::Notice(HubNotice::Session(told)) => told.context,
        _ => None,
    });
    assert_eq!(told, Some(after_prompts(1)));

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

/// The account the fake agent's CLI says it's logged in to, until a test says otherwise.
fn ada() -> AgentAccount {
    AgentAccount {
        // What the adapter makes of the fake's organization and email, the same in every build.
        id: Some("claude-f5d9122be47487b6".into()),
        logged_in: true,
        model_provider: Some("Anthropic API".to_string()),
        plan: Some("Max".to_string()),
        login: Some("Claude account".to_string()),
        email: Some("ada@example.com".to_string()),
    }
}

/// Has the fake agent's CLI say this of who's logged in, from here on.
fn log_in_as(stack: &Stack, status: &serde_json::Value) {
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("auth-status"), status.to_string()).unwrap();
}

/// What the hub has heard of the limits of the account the session's agent runs under, without when it heard it.
fn heard(client: &Client, session: &SessionId) -> Vec<UsageLimit> {
    let limits = client.session_account(session).unwrap().limits;
    limits.into_iter().map(|limit| UsageLimit { heard_at_ms: None, ..limit }).collect()
}

#[test]
fn a_chats_account_says_who_is_logged_in_and_its_limits_once_an_agent_under_it_has_worked() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let before = client.session_account(&session.id).unwrap();
    assert_eq!((before.account, before.limits), (ada(), Vec::new()), "no agent has heard of a limit yet");

    let asked_at_ms = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    // The hub asks which account the agent runs under once it first hears of a limit, then keeps what it heard.
    wait_until("the hub to keep what the agent heard", || heard(&client, &session.id) == [the_week()]);

    let after = client.session_account(&session.id).unwrap();
    assert_eq!(after.account, ada());
    assert!(after.limits[0].heard_at_ms.is_some_and(|at_ms| at_ms >= asked_at_ms), "{:?}", after.limits);
}

#[test]
fn every_chat_under_an_account_shares_its_limits_and_one_under_another_account_has_its_own() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, worked) = stack.workspace(&client);
    client.session_prompt(&worked.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &worked.id, 1);
    wait_until("the hub to keep what the agent heard", || heard(&client, &worked.id) == [the_week()]);

    // Another workspace has a worker of its own, whose agents have heard nothing, and the same login.
    let repo = client.repo_list().unwrap().remove(0);
    let other = client.workspace_create(&repo.id, Some("other".into()), None, None).unwrap();
    let idle = client.session_ensure(&other.id, AgentKind::Claude).unwrap();
    assert_eq!(heard(&client, &idle.id), [the_week()], "a limit is the account's, wherever it was heard");

    // Someone else in the same organization, so another account with the same model provider.
    log_in_as(
        &stack,
        &json!({"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty",
                "email": "grace@example.com", "orgId": "00000000-0000-0000-0000-000000000000"}),
    );
    let grace = client.session_account(&idle.id).unwrap();
    assert_eq!(grace.account.email.as_deref(), Some("grace@example.com"));
    assert_ne!(grace.account.id, ada().id);
    assert_eq!(grace.limits, [], "what Ada's agents heard isn't hers");
}

#[test]
fn nobody_logged_in_is_no_account_and_what_its_agent_hears_is_kept_for_nobody() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    log_in_as(&stack, &json!({"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}));

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));

    let account = client.session_account(&session.id).unwrap();
    assert!(!account.account.logged_in);
    assert_eq!((account.account.id, account.limits), (None, Vec::new()));
}

#[test]
fn a_chat_in_an_archived_workspace_cant_be_asked_its_account_so_asking_starts_no_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.workspace_archive(&workspace.id, true).unwrap();
    let workers = client.hub_status().unwrap().workers;

    let refused = client.session_account(&session.id).unwrap_err();

    assert!(matches!(&refused, ClientError::Api(e) if e.code == ErrorCode::Conflict), "{refused:?}");
    assert_eq!(client.hub_status().unwrap().workers, workers);
}
