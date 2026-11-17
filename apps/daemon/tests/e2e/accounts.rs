//! Whose account a chat's agent runs under, which is a record of its own, and what agents under it hear of its limits,
//! through a real hub, worker and agent's adapter. What the hub does with what a worker reads of the machine's login,
//! and with what an agent hears, is the hub's, and its own tests in `hub/accounts/tests/` prove it on stand-ins for its
//! workers. These are the whole path from the CLI's own words, a restart of the hub, and what the adapter asks the CLI.

#![cfg(test)]

use client_core::{Client, SubscriptionItem};
use protocol::client::HubNotice;
use protocol::event::EventBody;
use protocol::model::{
    Account, AccountId, AgentAccount, LimitWindow, ModelLabel, Seq, SessionId, SessionStatus, UsageLimit,
};
use serde_json::json;

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
