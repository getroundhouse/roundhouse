//! The logins a hub holds for agents' accounts, through a real hub that restarts.

#![cfg(test)]

use client_core::Client;
use protocol::client::accounts::{AccountConnect, AccountDisconnect};
use protocol::model::{Account, AccountId, AgentKind, LoginToken};

use crate::support::Stack;

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
    let token = LoginToken::new("a-made-up-token");
    let connect = AccountConnect { agent: AgentKind::Claude, name: "Work".to_string(), token, account: None };
    let connected = stack.client().call(connect).unwrap();
    // As a hub that stopped part way through connecting another would leave one.
    std::fs::write(stack.paths().logins_dir.join("00000000-0000-4000-8000-000000000000"), "a-half-kept-token").unwrap();

    stack.restart_hub();
    let client = stack.client();
    let after_restart = listed(&client, &connected.id);
    let kept_after_restart = tokens_kept(&stack);
    let disconnected = client.call(AccountDisconnect { id: connected.id }).unwrap();

    assert_eq!(connected.login_agent, Some(AgentKind::Claude));
    assert!(connected.held_login.is_some());
    assert_eq!(after_restart.held_login, connected.held_login);
    assert_eq!(kept_after_restart, ["a-made-up-token"], "the file no account names went as the hub started");
    assert_eq!((disconnected.held_login, disconnected.name.as_deref()), (None, Some("Work")));
    assert_eq!(tokens_kept(&stack), Vec::<String>::new());
}
