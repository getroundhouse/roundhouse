//! What a running agent has of a tool server the person edits, here a stand-in for one in the test's process.

#![cfg(test)]

use std::path::PathBuf;

use client_core::Client;
use protocol::capability::settings::Settings;
use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::event::EventBody;
use protocol::model::{
    AgentKind, Observation, Observed, SessionId, SessionStatus, ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey,
    ToolServerUrl,
};

use crate::support::{Stack, calls, session_records, wait_for_status, wait_for_turn_end, wait_until};
use crate::tool_servers::{ANOTHER, CALL, KEY, Took, add, holding, stand_in};

/// Gives the server [`ANOTHER`] key, at `url`.
fn give_another_key(client: &Client, url: &str, moves: bool) {
    let key = Some(ToolServerKey::new(ANOTHER));
    let auth = Some(ToolServerAuthEdit::Key { header: "X-Key".into(), key, url: url.into() });
    client
        .call(ToolServerEdit { id: "t".into(), name: None, url: moves.then(|| url.into()), auth, saved_anyway: false })
        .unwrap();
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

/// Its worker is set the rest while it runs, and the agent, which was connected to where the server was, is refused in its
/// turn, then connects to where it is, so neither server has anything that's the other's.
#[test]
fn a_running_agent_is_cut_off_from_a_server_thats_moved_until_its_turn_ends_and_then_connected_to_it_there() {
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
    let gate = stack.path("moved");
    client.session_prompt(&chat.id, &format!("hang-until:{}; {CALL}", gate.display()), None).unwrap();
    wait_for_status(&client, &chat.id, SessionStatus::Running);

    give_another_key(&client, &moved, true);
    wait_until("it to wait for the turn to end", || {
        said(&client, &chat.id).connects_again == Some(ConnectsAgain::WhenItsTurnEnds)
    });
    assert_eq!(heard_moved.try_iter().count(), 0);
    std::fs::write(gate, "").unwrap();

    let events = wait_for_turn_end(&client, &chat.id, 2);
    let results = calls::results(&events);
    let (why, failed) = results.last().unwrap();
    assert!(*failed && why.contains("has been moved to another URL"), "{why}");
    wait_until("it to be connected there", || {
        let said = said(&client, &chat.id);
        said.connected_to == place(&moved) && said.state == ServerState::Connected && said.connects_again.is_none()
    });
    assert!(heard_moved.try_iter().all(|took| took.key == ANOTHER && took.method != "tools/call"));
    assert_eq!(calls::run(&client, &chat.id, CALL, 3), works);
    let call = Took { method: "tools/call".into(), key: ANOTHER.into(), path: "/mcp".into() };
    assert_eq!(heard_moved.try_iter().last(), Some(call));
    assert!(heard.try_iter().all(|took| took.key == KEY), "the old server has nothing of the new one's");
    let events = wait_for_turn_end(&client, &chat.id, 3);
    let started = events.iter().filter(|event| matches!(event.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the same agent");
}

/// The stand-in refuses the key it's added with, so the agent failed to connect to it.
#[test]
fn a_running_agent_is_connected_to_a_failed_server_once_its_key_is_corrected() {
    let (url, _heard) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    let auth = ToolServerAuthGiven::Key { header: "X-Key".into(), key: ToolServerKey::new("made-up-wrong-key") };
    let adding = ToolServerAdd { id: "t".into(), name: "Linear (Northwind)".into(), url: url.clone(), auth };
    client.call(adding).unwrap();
    let (_, chat) = stack.workspace(&client);
    assert!(calls::run(&client, &chat.id, CALL, 1)[0].1);
    wait_until("it to have failed", || said(&client, &chat.id).state == ServerState::Failed);

    give_another_key(&client, &url, false);

    wait_until("it to be connected", || said(&client, &chat.id).state == ServerState::Connected);
    assert_eq!(calls::run(&client, &chat.id, CALL, 2), [("Found issue 7".to_string(), false)]);
}

/// One on none lists them at once, which says that the hub has looked at the other too, whose call has the query by then.
#[test]
fn a_running_agent_lists_a_servers_tools_by_its_other_query_once_its_on_no_turn() {
    let (url, heard) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    add(&client, url.clone());
    let (workspace, busy) = stack.workspace(&client);
    let idle = client.session_create(&workspace.id, AgentKind::Claude, Settings::default(), None).unwrap();
    for chat in [&busy, &idle] {
        assert_eq!(calls::run(&client, &chat.id, CALL, 1), [("Found issue 7".to_string(), false)]);
        wait_until("it to be connected", || said(&client, &chat.id).state == ServerState::Connected);
    }
    let gate = stack.path("queried");
    client.session_prompt(&busy.id, &format!("hang-until:{}; {CALL}", gate.display()), None).unwrap();
    wait_for_status(&client, &busy.id, SessionStatus::Running);

    let url = Some(format!("{url}?tools=some"));
    client.call(ToolServerEdit { id: "t".into(), name: None, url, auth: None, saved_anyway: false }).unwrap();
    let mut took = Vec::new();
    let mut lists = |times: usize| {
        wait_until("its tools to be listed", || {
            let queried = heard.try_iter().filter(|took| took.path == "/mcp?tools=some");
            took.extend(queried.map(|took| took.method).filter(|method| method.starts_with("tools/")));
            took.iter().filter(|method| *method == "tools/list").count() == times
        });
        took.clone()
    };
    assert_eq!(lists(1), ["tools/list"]);
    std::fs::write(gate, "").unwrap();

    assert_eq!(lists(2), ["tools/list", "tools/call", "tools/list"], "in its turn, and once that has ended");
}
