//! Whose account a chat's agent runs under, which is a record of its own, and what agents under it hear of its limits.

#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use client_core::{Client, ClientError, SubscriptionItem};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::EventBody;
use protocol::model::{
    Account, AccountId, AgentAccount, AgentKind, LimitWindow, ModelLabel, Seq, SessionId, SessionStatus, UsageLimit,
};
use serde_json::json;

use crate::context::{after_prompts, wait_for_context};
use crate::support::{Stack, TIMEOUT, wait_for_status, wait_for_turn_end, wait_until};

/// The week's limit the fake agent hears of in its process's first turn, and says when asked.
pub(crate) fn the_week() -> UsageLimit {
    UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 4,
        resets_at_ms: Some(4_102_444_800_000),
        heard_at_ms: None,
    }
}

/// Fable's own week, which the fake agent says only when asked, since its line of what's left names no model.
fn fables_week() -> UsageLimit {
    UsageLimit {
        window: LimitWindow::ModelSevenDays,
        model: Some(ModelLabel::new("Fable")),
        used_percent: 12,
        ..the_week()
    }
}

/// The account the fake agent's CLI says it's logged in to, until a test says otherwise.
pub(crate) fn ada() -> AgentAccount {
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
pub(crate) fn log_in_as(stack: &Stack, status: &serde_json::Value) {
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("auth-status"), status.to_string()).unwrap();
}

/// Has the fake agent's CLI say nobody is logged in on its machine, from here on. How the real CLI says so hasn't been
/// seen, as `docs/claude-cli.md` says, so this is the adapter's reading of it: `loggedIn` false.
pub(crate) fn log_out(stack: &Stack) {
    log_in_as(stack, &json!({"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}));
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
    wait_for_adas_limits(client, session, &[the_week()]);
}

/// Waits until the session names Ada's account, and its record has `limits`.
fn wait_for_adas_limits(client: &Client, session: &SessionId, limits: &[UsageLimit]) {
    wait_until("the account's record to have its limits", || {
        named_account(client, session)
            .is_some_and(|account| Some(&account.id) == ada().id.as_ref() && account.limits == limits)
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
    log_out(&stack);

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
fn a_models_week_an_answer_no_longer_lists_is_forgotten_as_the_one_it_lists_is_kept() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // As the real CLI after its first turn, which says what's left only once a process, so each launch is asked.
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("limits-unsaid"), "").unwrap();
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_adas_limits(&client, &session.id, &[the_week(), fables_week()]);
    client.session_prompt(&session.id, "crash:3", None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);

    // The provider labels the model anew, as for a newer version of it.
    std::fs::write(stack.agent_state().join("model-label"), "Fable 5.1").unwrap();
    client.session_prompt(&session.id, "say:again", None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);

    let renamed = UsageLimit { model: Some(ModelLabel::new("Fable 5.1")), ..fables_week() };
    wait_for_adas_limits(&client, &session.id, &[the_week(), renamed]);
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

    // The answer has Fable's own week too, by the model's label, which the line of what's left doesn't say.
    wait_for_adas_limits(&client, &session.id, &[the_week(), fables_week()]);
    // It answers the adapter's own question, like how full the context is, and the answer says how the account was
    // used, so none of it is in the log.
    let (events, _) = client.session_read(&session.id, Seq::new(1), 500, true).unwrap();
    assert!(events.iter().all(|e| !matches!(e.body, EventBody::AgentLimits { .. })), "{events:?}");
    assert!(events.iter().all(|e| !e.raw.as_deref().unwrap_or_default().contains("get_usage")));
}
