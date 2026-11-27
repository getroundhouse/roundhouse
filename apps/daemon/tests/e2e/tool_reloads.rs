//! Having a chat's agent connect again to one of its tool servers, which a stand-in in the test's process is, while the
//! agent runs.

#![cfg(test)]

use std::net::{Ipv4Addr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use client_core::{ClientError, Connection};
use protocol::capability::setup::ServerState;
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::SessionId;

use crate::support::{Stack, calls, read_all, wait_until};
use crate::tool_servers::{CALL, Took, add, serve, states};

/// A stand-in as `tool_servers` has, that answers nothing while it's down.
struct Server {
    up: Arc<AtomicBool>,
    url: String,
    took: Receiver<Took>,
}

impl Server {
    /// One that answers from the start if `up`.
    fn new(up: bool) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let up = Arc::new(AtomicBool::new(up));
        let (answers, (takes, took)) = (Arc::clone(&up), channel());
        std::thread::spawn(move || {
            for stream in listener.incoming().filter(|_| answers.load(Ordering::Acquire)) {
                let takes = takes.clone();
                std::thread::spawn(move || serve(&stream.unwrap(), &takes));
            }
        });
        Self { up, url, took }
    }

    fn set_up(&self, up: bool) {
        self.up.store(up, Ordering::Release);
    }
}

/// A stack whose one chat has `server`, as `Linear_Northwind`.
fn chat_with(server: &Server) -> (Stack, Connection, SessionId) {
    let stack = Stack::start();
    let client = stack.client();
    add(&client, server.url.clone());
    let (_, chat) = stack.workspace(&client);
    (stack, client, chat.id)
}

/// Waits until the chat's agent says the server is `state`.
fn comes_to(client: &Connection, chat: &SessionId, state: ServerState) {
    let is = [("Linear_Northwind".to_string(), state)];
    wait_until("what comes of it", || states(client, chat) == is);
}

fn reload(client: &Connection, chat: &SessionId) -> Result<(), ClientError> {
    client.session_reload_tool_server(chat, &"Linear_Northwind".into())
}

/// Refused while its agent isn't running, failed as the agent launched, and connected to once it's reloaded, by the
/// agent that's running.
#[test]
fn a_server_that_failed_as_its_agent_launched_is_connected_to_once_its_reloaded() {
    let server = Server::new(false);
    let (_stack, client, chat) = chat_with(&server);

    let Err(ClientError::Api(refused)) = reload(&client, &chat) else { panic!("its agent isn't running") };
    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert!(calls::run(&client, &chat, CALL, 1)[0].1);
    comes_to(&client, &chat, ServerState::Failed);
    server.set_up(true);
    reload(&client, &chat).unwrap();
    comes_to(&client, &chat, ServerState::Connected);

    assert_eq!(calls::run(&client, &chat, CALL, 2), [("Found issue 7".to_string(), false)]);
    let events = read_all(&client, &chat);
    let started = events.iter().filter(|event| matches!(event.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the same agent");
}

/// Its tools are listed again, though it's connected before and after.
#[test]
fn a_server_thats_connected_has_its_tools_read_again_as_its_reloaded() {
    let server = Server::new(true);
    let (_stack, client, chat) = chat_with(&server);
    assert_eq!(calls::run(&client, &chat, CALL, 1), [("Found issue 7".to_string(), false)]);
    server.took.try_iter().for_each(drop);

    reload(&client, &chat).unwrap();

    wait_until("its tools to be read again", || server.took.try_iter().any(|took| took.method == "tools/list"));
}

/// Lost when it's reloaded as it's down, and connected to again when it's reloaded once it's up.
#[test]
fn a_server_reloaded_as_its_down_is_lost_until_its_reloaded_once_its_up() {
    let server = Server::new(true);
    let (_stack, client, chat) = chat_with(&server);
    assert_eq!(calls::run(&client, &chat, CALL, 1), [("Found issue 7".to_string(), false)]);
    comes_to(&client, &chat, ServerState::Connected);

    server.set_up(false);
    reload(&client, &chat).unwrap();
    comes_to(&client, &chat, ServerState::Failed);
    server.set_up(true);
    reload(&client, &chat).unwrap();

    comes_to(&client, &chat, ServerState::Connected);
}
