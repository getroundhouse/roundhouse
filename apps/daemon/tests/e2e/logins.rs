//! The logins a hub holds for agents' accounts, through a real hub, and the agents it launches under them on a machine
//! with no login of its own, on any provider.

#![cfg(test)]

use client_core::{Client, Subscription, SubscriptionItem};
use protocol::client::HubNotice;
use protocol::event::EventBody;
use protocol::model::{Account, AccountId, AgentKind, LoginToken};

use crate::accounts::{ada, log_out};
use crate::carrying_on::restart;
use crate::support::{Stack, TIMEOUT, assistant_text, wait_for_turn_end, wait_until};

fn listed(client: &Client, id: &AccountId) -> Account {
    client.records_list().unwrap().accounts.into_iter().find(|account| account.id == *id).unwrap()
}

/// Every file in the hub's logins directory, by what's in it.
fn tokens_kept(stack: &Stack) -> Vec<String> {
    let entries = std::fs::read_dir(stack.paths().logins_dir).unwrap();
    let mut tokens: Vec<_> = entries.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap()).collect();
    tokens.sort();
    tokens
}

#[test]
fn a_login_a_client_connects_is_kept_through_a_restart_until_its_disconnected() {
    let mut stack = Stack::start();
    let connected = connect_work(&stack.client());
    // As a hub that stopped part way through connecting another would leave one.
    std::fs::write(stack.paths().logins_dir.join("00000000-0000-4000-8000-000000000000"), "a-half-kept-token").unwrap();

    stack.restart_hub();
    let client = stack.client();
    let after_restart = listed(&client, &connected.id);
    let kept_after_restart = tokens_kept(&stack);
    let disconnected = client.account_disconnect(&connected.id).unwrap();

    assert_eq!(connected.login_agent, Some(AgentKind::Claude));
    assert!(connected.held_login.is_some());
    assert_eq!(after_restart.held_login, connected.held_login);
    assert_eq!(kept_after_restart, ["a-made-up-token"], "the file no account names went as the hub started");
    assert_eq!((disconnected.held_login, disconnected.name.as_deref()), (None, Some("Work")));
    assert_eq!(tokens_kept(&stack), Vec::<String>::new());
}

/// Connects a made-up Claude login, named Work.
fn connect_work(client: &Client) -> Account {
    client.account_connect(AgentKind::Claude, "Work", LoginToken::new("a-made-up-token"), None).unwrap()
}

/// Whether anything asked the fake agent's CLI who's logged in while it was given a token, which only the hub's record
/// can say whose it is. Asking with none, as the hub does before a launch to learn whether the machine has a login of
/// its own, is another matter.
fn asked_under_a_token(stack: &Stack) -> bool {
    let calls = std::fs::read_to_string(stack.agent_state().join("auth-status-calls")).unwrap_or_default();
    calls.lines().any(|call| call.starts_with("asked under"))
}

/// Through an adapter, as on another provider's machine, which has no login of its own.
#[test]
fn an_agent_on_another_machine_runs_under_the_login_the_hub_holds_and_its_chat_names_that_account() {
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    log_out(&stack);
    let client = stack.client();
    let connected = connect_work(&client);
    let (_, session) = stack.workspace(&client);

    client.session_prompt(&session.id, "env:CLAUDE_CODE_OAUTH_TOKEN", None).unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    wait_until("the chat to name the login's account, and the account what its agent heard of its limits", || {
        client.session_get(&session.id).unwrap().account.as_ref() == Some(&connected.id)
            && !listed(&client, &connected.id).limits.is_empty()
    });
    // As a client from before accounts were records asks.
    let as_older_clients_ask = client.session_account(&session.id).unwrap();

    assert!(text.contains("CLAUDE_CODE_OAUTH_TOKEN=a-made-up-token"), "{text}");
    assert_eq!(client.records_list().unwrap().accounts.len(), 1, "no account but the login's");
    assert_eq!(as_older_clients_ask.account.logged_in_as(), Some(&connected.id));
    assert_eq!(as_older_clients_ask.limits, listed(&client, &connected.id).limits);
    assert!(!asked_under_a_token(&stack), "nothing asked the CLI whose the token is");
}

/// The case the hub's record of each run's login is for: the next hub carries the agent on, and nothing can say whose
/// its token is but that record.
#[test]
fn an_agent_left_running_under_a_held_login_is_still_filed_under_it_once_the_hub_has_restarted() {
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    log_out(&stack);
    let client = stack.client();
    let connected = connect_work(&client);
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    restart(&mut stack);
    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:again", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);

    let launches = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(launches, 1, "the agent the last hub launched carried on");
    assert_eq!(client.session_get(&session.id).unwrap().account, Some(connected.id));
    assert_eq!(client.records_list().unwrap().accounts.len(), 1, "no account but the login's");
    assert!(!asked_under_a_token(&stack), "nothing asked the CLI whose the token is");
}

/// When the feed next announces the account `id` as used after `after_ms`.
fn announced_used_after(feed: &Subscription, id: &AccountId, after_ms: i64) -> Option<i64> {
    std::iter::from_fn(|| feed.items.recv_timeout(TIMEOUT).ok()).find_map(|item| match item {
        SubscriptionItem::Notice(notice) => match *notice {
            HubNotice::Account(told) if told.id == *id => told.used_at_ms.filter(|used| *used > after_ms),
            _ => None,
        },
        _ => None,
    })
}

/// On this machine, through the local provider, as for a hub on a server the person never logged in on, which hands
/// the login they gave it to every workspace. The feed tells every client when the account was last used: by the first
/// prompt, once the launch it brought names the account, and then by each prompt sent. The agent hears of no limits,
/// so the account is announced for nothing else.
#[test]
fn an_agent_on_this_machine_runs_under_the_login_the_hub_holds_when_the_machine_has_none_of_its_own() {
    let stack = Stack::start();
    log_out(&stack);
    std::fs::write(stack.agent_state().join("no-limits"), "").unwrap();
    let client = stack.client();
    let connected = connect_work(&client);
    let (_, session) = stack.workspace(&client);
    let feed = client.feed_subscribe().unwrap();

    client.session_prompt(&session.id, "env:CLAUDE_CODE_OAUTH_TOKEN", None).unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    let first = announced_used_after(&feed, &connected.id, 0);
    client.session_prompt(&session.id, "say:again", None).unwrap();
    wait_for_turn_end(&client, &session.id, 2);
    let second = first.and_then(|first| announced_used_after(&feed, &connected.id, first));

    assert!(text.contains("CLAUDE_CODE_OAUTH_TOKEN=a-made-up-token"), "{text}");
    assert_eq!(client.session_get(&session.id).unwrap().account, Some(connected.id.clone()));
    assert!(first.is_some(), "the first prompt was announced as a use");
    assert!(second.is_some(), "and so was the next");
    assert_eq!(client.records_list().unwrap().accounts.len(), 1, "no account but the login's");
}

/// Through an adapter, as on another provider's machine the person logged in on, which keeps its own login whatever
/// the hub holds.
#[test]
fn an_agent_on_a_machine_with_its_own_login_runs_under_that_though_the_hub_holds_one() {
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    let client = stack.client();
    let connected = connect_work(&client);
    let (_, session) = stack.workspace(&client);

    client.session_prompt(&session.id, "env:CLAUDE_CODE_OAUTH_TOKEN", None).unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    let adas = ada().id;
    wait_until("the chat to name the machine's own account", || {
        client.session_get(&session.id).unwrap().account == adas
    });

    assert!(text.contains("CLAUDE_CODE_OAUTH_TOKEN=<unset>"), "{text}");
    assert_eq!(listed(&client, &connected.id).used_at_ms, None, "the login the hub holds wasn't used");
}
