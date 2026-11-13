//! How full an agent's context is, on its chat's record, and whose account it runs under, which is a record of its
//! own.

#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use client_core::{Client, ClientError, SubscriptionItem};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::EventBody;
use protocol::model::{
    Account, AccountId, AgentAccount, AgentKind, ContextPart, ContextPartKind, ContextUsage, LimitWindow, MaxSetBy,
    SessionId, SessionStatus, UsageLimit,
};
use serde_json::json;

use crate::support::{Stack, TIMEOUT, wait_for_status, wait_for_turn_end, wait_until};

/// What the fake agent says its context holds once a conversation has seen this many prompts, and a turn has ended
/// since it launched. Its settings make the context a fifth of what its model holds, which only a turn's end says.
fn after_prompts(prompts: u64) -> ContextUsage {
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

/// The week's limit the fake agent hears of in its process's first turn, and says when asked.
fn the_week() -> UsageLimit {
    UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
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

/// What a client from before accounts were records asks of a chat's account, which the hub still answers.
#[test]
fn a_chats_account_and_its_limits_can_still_be_asked_for_by_an_older_client() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let before = client.session_account(&session.id).unwrap();
    assert_eq!((before.account, before.limits), (ada(), Vec::new()), "no agent has heard of a limit yet");

    let asked_at_ms = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let heard = || {
        let limits = client.session_account(&session.id).unwrap().limits;
        limits.into_iter().map(|limit| UsageLimit { heard_at_ms: None, ..limit }).collect::<Vec<_>>()
    };
    wait_until("the hub to keep what the agent heard", || heard() == [the_week()]);

    let after = client.session_account(&session.id).unwrap();
    assert_eq!(after.account, ada());
    assert!(after.limits[0].heard_at_ms.is_some_and(|at_ms| at_ms >= asked_at_ms), "{:?}", after.limits);
}

/// Even once it names the account its agent last launched under, since only the worker says whose a login on its
/// machine is.
#[test]
fn a_chat_in_an_archived_workspace_cant_be_asked_its_account_so_asking_starts_no_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_adas_week(&client, &session.id);
    client.workspace_archive(&workspace.id, true).unwrap();
    let workers = client.hub_status().unwrap().workers;

    let refused = client.session_account(&session.id).unwrap_err();

    assert!(matches!(&refused, ClientError::Api(e) if e.code == ErrorCode::Conflict), "{refused:?}");
    assert_eq!(client.hub_status().unwrap().workers, workers);
}

/// The record of the account `id`.
fn account(client: &Client, id: &AccountId) -> Option<Account> {
    client.records_list().unwrap().accounts.into_iter().find(|account| &account.id == id)
}

/// The record of the account the session names.
fn named_account(client: &Client, session: &SessionId) -> Option<Account> {
    account(client, &client.session_get(session).unwrap().account?)
}

/// Waits until the session names Ada's account, and its record has the week's limit.
fn wait_for_adas_week(client: &Client, session: &SessionId) {
    wait_until("the account's record to have the week's limit", || {
        named_account(client, session)
            .is_some_and(|account| Some(&account.id) == ada().id.as_ref() && account.limits == [the_week()])
    });
}

#[test]
fn a_chat_names_the_account_its_agent_launched_under_and_the_feed_announces_what_its_agents_hear() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    assert_eq!(session.account, None, "no agent has launched yet");
    let feed = client.feed_subscribe().unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_adas_week(&client, &session.id);

    let account = named_account(&client, &session.id).unwrap();
    assert_eq!(Some(account.id.clone()), ada().id);
    assert_eq!((account.email, account.plan), (ada().email, ada().plan), "who it is, as the worker read it");
    // The feed says so, so a client that isn't asking sees the limits as soon as they're heard.
    let told = std::iter::from_fn(|| feed.items.recv_timeout(TIMEOUT).ok()).find_map(|item| match item {
        SubscriptionItem::Notice(notice) => match *notice {
            HubNotice::Account(told) if !told.limits.is_empty() => Some(told),
            _ => None,
        },
        _ => None,
    });
    assert_eq!(told.map(|told| told.id), ada().id);
}

#[test]
fn a_chat_names_its_account_as_soon_as_its_agent_launches_before_any_limit_is_heard() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("no-limits"), "").unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_until("the chat to name its account", || client.session_get(&session.id).unwrap().account.is_some());

    let account = named_account(&client, &session.id).unwrap();
    assert_eq!((Some(account.id), account.email), (ada().id, ada().email), "whose account it is, from the launch");
    assert_eq!(account.limits, [], "and nothing heard of its limits");
}

#[test]
fn chats_in_two_workspaces_under_one_login_name_one_account_with_one_set_of_limits() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, first) = stack.workspace(&client);
    let repo = client.repo_list().unwrap().remove(0);
    let other = client.workspace_create(&repo.id, Some("other".into()), None, None).unwrap();
    let second = client.session_ensure(&other.id, AgentKind::Claude).unwrap();

    for (session, turns) in [(&first, 1), (&second, 1)] {
        client.session_prompt(&session.id, "say:hello", None).unwrap();
        wait_for_turn_end(&client, &session.id, turns);
        wait_until("the chat to name its account", || client.session_get(&session.id).unwrap().account.is_some());
    }

    assert_eq!(client.session_get(&first.id).unwrap().account, client.session_get(&second.id).unwrap().account);
    assert_eq!(client.records_list().unwrap().accounts.len(), 1, "a limit is the account's, wherever it was heard");
}

#[test]
fn a_chat_still_names_its_account_and_the_account_its_limits_after_the_hub_restarts() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_adas_week(&client, &session.id);

    stack.kill_hub();
    stack.start_hub();

    let after = named_account(&stack.client(), &session.id).expect("the chat still names it");
    assert_eq!((Some(after.id), after.limits), (ada().id, vec![the_week()]));
}

#[test]
fn a_chat_whose_agent_launched_with_nobody_logged_in_names_no_account() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    log_in_as(&stack, &json!({"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}));

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));

    assert_eq!(client.session_get(&session.id).unwrap().account, None);
    assert_eq!(client.records_list().unwrap().accounts, []);
}

#[test]
fn a_chat_whose_agent_relaunches_under_another_login_names_that_account_and_the_first_keeps_its_limits() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_adas_week(&client, &session.id);
    client.session_prompt(&session.id, "crash:3", None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);

    // Someone else in the same organization, so another account with the same model provider, whose agent hears
    // nothing of its limits.
    log_in_as(
        &stack,
        &json!({"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty",
                "email": "grace@example.com", "orgId": "00000000-0000-0000-0000-000000000000"}),
    );
    std::fs::write(stack.agent_state().join("no-limits"), "").unwrap();
    client.session_prompt(&session.id, "say:again", None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);
    wait_until("the chat to name the account its agent relaunched under", || {
        named_account(&client, &session.id).is_some_and(|account| account.id != ada().id.unwrap())
    });

    let grace = named_account(&client, &session.id).unwrap();
    assert_eq!((grace.email.as_deref(), grace.limits), (Some("grace@example.com"), vec![]), "Ada's limits aren't hers");
    assert_eq!(account(&client, &ada().id.unwrap()).unwrap().limits, [the_week()]);
}

#[test]
fn a_launchs_account_is_asked_again_when_the_worker_couldnt_say_and_what_its_agent_heard_meanwhile_is_kept() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("auth-status"), "not what the CLI says").unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let calls = stack.agent_state().join("auth-status-calls");
    wait_until("the worker to have failed to say", || calls.exists());
    std::fs::remove_file(stack.agent_state().join("auth-status")).unwrap();

    // The week's limit was heard in the turn, before anyone knew whose it was.
    wait_for_adas_week(&client, &session.id);
}

#[test]
fn an_agent_that_doesnt_say_its_limits_by_itself_is_asked_and_the_answer_stays_out_of_the_log() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // As the real CLI after its first turn, which says what's left only once a process.
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("limits-unsaid"), "").unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    wait_for_adas_week(&client, &session.id);
    // It answers the adapter's own question, like how full the context is, and the answer says how the account was
    // used, so none of it is in the log.
    let (events, _) = client.session_read(&session.id, 1, 500, true).unwrap();
    assert!(events.iter().all(|e| !matches!(e.body, EventBody::AgentLimits { .. })), "{events:?}");
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("get_usage")));
}
