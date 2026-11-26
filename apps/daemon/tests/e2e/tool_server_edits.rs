//! What a running agent has of a tool server the person edits, here a stand-in for one in the test's process.

#![cfg(test)]

use std::path::PathBuf;

use client_core::Client;
use protocol::capability::setup::ToolServerState;
use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::{
    Observation, Observed, SessionId, SessionStatus, ToolServerAuthEdit, ToolServerKey, ToolServerUrl,
};

use crate::support::{Stack, calls, session_records, wait_for_status, wait_until};
use crate::tool_servers::{ANOTHER, CALL, KEY, add, holding, stand_in};

/// Gives the server [`ANOTHER`] key, at `url`.
fn give_another_key(client: &Client, url: &str, moves: bool) {
    let key = Some(ToolServerKey::new(ANOTHER));
    let auth = Some(ToolServerAuthEdit::Key { header: "X-Key".into(), key, url: url.into() });
    client.call(ToolServerEdit { id: "t".into(), name: None, url: moves.then(|| url.into()), auth }).unwrap();
}

/// The only server the chat's agent last said it has.
fn said(client: &Client, chat: &SessionId) -> ToolServerState {
    let listed = session_records(client, chat).observations.into_iter();
    let servers = listed.filter_map(|observation| match observation {
        Observation::SessionToolServers { servers: Observed::AsOf { value, .. }, .. } => Some(value),
        _ => None,
    });
    let servers: Vec<_> = servers.flatten().collect();
    let [server] = &servers[..] else { panic!("{servers:?}") };
    server.clone()
}

#[test]
fn a_running_agents_calls_have_a_servers_other_key_at_once_and_the_one_before_is_forgotten() {
    let (url, heard) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    add(&client, url.clone());
    let (_, chat) = stack.workspace(&client);
    let works = [("Found issue 7".to_string(), false)];
    assert_eq!(calls::run(&client, &chat.id, CALL, 1), works);
    assert!(heard.try_iter().all(|took| took.key == KEY));

    give_another_key(&client, &url, false);

    let mut turn = 1;
    wait_until("a call to have the other key", || {
        turn += 1;
        assert_eq!(calls::run(&client, &chat.id, CALL, turn), works);
        heard.try_iter().last().unwrap().key == ANOTHER
    });
    assert_eq!(holding(&stack.path(""), KEY), Vec::<PathBuf>::new());
    assert_eq!(holding(&stack.path(""), ANOTHER).len(), 1);
}

/// Its worker is set the rest while it runs, and the agent was connected to where the server was.
#[test]
fn a_running_agent_is_cut_off_from_a_server_thats_moved_until_its_next_start() {
    let (url, heard) = stand_in();
    let (moved, heard_moved) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    add(&client, url.clone());
    let (_, chat) = stack.workspace(&client);
    let works = [("Found issue 7".to_string(), false)];
    let place = |url: &str| Some(ToolServerUrl::typed(url).unwrap().place());
    assert_eq!(calls::run(&client, &chat.id, CALL, 1), works);
    wait_until("where it's connected", || said(&client, &chat.id).connected_to == place(&url));

    give_another_key(&client, &moved, true);

    let (mut turn, mut answered) = (1, Vec::new());
    wait_until("a call to fail", || {
        turn += 1;
        answered = calls::run(&client, &chat.id, CALL, turn);
        answered.iter().any(|(_, is_error)| *is_error)
    });
    let why = "Linear (Northwind) has been moved to another URL in Roundhouse since this agent connected to it";
    assert!(answered[0].0.contains(why), "{answered:?}");
    assert!(heard.try_iter().all(|took| took.key == KEY), "the old server has nothing of the new one's");
    assert_eq!(heard_moved.try_iter().last(), None, "nor the new one of the old one's");
    assert_eq!(said(&client, &chat.id).connected_to, place(&url), "as its card says");

    client.session_prompt(&chat.id, "crash:3", None).unwrap();
    wait_for_status(&client, &chat.id, SessionStatus::Interrupted);

    assert_eq!(calls::run(&client, &chat.id, CALL, turn + 1), works);
    assert!(heard_moved.try_iter().all(|took| took.key == ANOTHER));
    wait_until("where it's connected", || said(&client, &chat.id).connected_to == place(&moved));
}
