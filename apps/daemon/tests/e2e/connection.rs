//! The connection to the hub.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;

use client_core::{Client, ClientError, Ended, HubAdmin};
use protocol::client::{ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::event::EventBody;
use protocol::model::{AgentKind, DiffSide, SessionSettingsChange};

use crate::support::{Follower, Stack, TIMEOUT, assert_gapless, terminate, wait_for_turn_end, wait_until};

#[test]
fn a_client_knows_which_build_its_hub_is() {
    let stack = Stack::start();

    let client = stack.client();

    assert_eq!(client.hub_version(), protocol::BUILD_VERSION);
    assert_eq!(client.hub_status().unwrap().hub_version, protocol::BUILD_VERSION);
}

/// A connection that sends frames as written here, for what `Client` has no way to send.
struct RawConnection {
    socket: UnixStream,
    replies: BufReader<UnixStream>,
}

impl RawConnection {
    fn open(stack: &Stack) -> Self {
        let socket = UnixStream::connect(stack.socket()).unwrap();
        socket.set_read_timeout(Some(TIMEOUT)).unwrap();
        let replies = BufReader::new(socket.try_clone().unwrap());
        Self { socket, replies }
    }

    fn ask(&mut self, id: u64, request: &str) -> ReplyOutcome {
        self.ask_unless_closed(id, request)
            .unwrap_or_else(|| panic!("the hub closed the connection before replying to {id}"))
    }

    /// `None` if the hub closes the connection before it replies.
    fn ask_unless_closed(&mut self, id: u64, request: &str) -> Option<ReplyOutcome> {
        writeln!(self.socket, r#"{{"id":{id},"request":{request}}}"#).unwrap();
        let mut line = String::new();
        if self.replies.read_line(&mut line).unwrap() == 0 {
            return None;
        }
        match protocol::frame::decode(line.as_bytes()).unwrap() {
            HubFrame::Reply { id: answered, outcome } if answered == id => Some(outcome),
            other => panic!("not the reply to {id}: {other:?}"),
        }
    }
}

fn hello() -> String {
    format!(r#"{{"method":"hello","client":"test","protocol_version":{}}}"#, protocol::PROTOCOL_VERSION)
}

#[test]
fn a_hub_refuses_a_request_it_does_not_know_and_carries_on() {
    let stack = Stack::start();
    let mut connection = RawConnection::open(&stack);
    assert!(matches!(connection.ask(1, &hello()), ReplyOutcome::Response(ClientResponse::Hello { .. })));

    let refused = connection.ask(2, r#"{"method":"from_a_newer_client","path":"a"}"#);

    assert!(matches!(refused, ReplyOutcome::Error(e) if e.code == ErrorCode::Unsupported));
    let still_there = connection.ask(3, r#"{"method":"hub_status"}"#);
    assert!(matches!(still_there, ReplyOutcome::Response(ClientResponse::HubStatus(_))));
}

/// A client across a real break sends these as written, so a test can't build them from today's types.
#[test]
fn a_hub_shows_its_status_and_stops_without_a_hello() {
    let mut stack = Stack::start();
    let mut connection = RawConnection::open(&stack);

    let status = connection.ask(1, r#"{"method":"hub_status"}"#);

    assert!(matches!(status, ReplyOutcome::Response(ClientResponse::HubStatus(s)) if s.pid == stack.hub_pid()));
    let stopped = connection.ask_unless_closed(2, r#"{"method":"hub_shutdown"}"#);
    // It can stop before its reply gets out, which a client takes as done, as `Client::hub_shutdown` does.
    assert!(matches!(stopped, None | Some(ReplyOutcome::Response(ClientResponse::Ok))), "{stopped:?}");
    stack.hub.take().unwrap().wait().unwrap();
}

#[test]
fn nothing_else_works_without_a_hello() {
    let stack = Stack::start();
    let mut connection = RawConnection::open(&stack);

    let refused = connection.ask(1, r#"{"method":"repo_list"}"#);

    assert!(matches!(refused, ReplyOutcome::Error(e) if e.code == ErrorCode::VersionMismatch));
}

#[test]
fn a_refused_client_can_still_see_the_hub_on_a_new_connection() {
    let stack = Stack::start();
    let mut refused = RawConnection::open(&stack);
    let hello = r#"{"method":"hello","client":"newer","protocol_version":9999}"#;
    assert!(matches!(refused.ask(1, hello), ReplyOutcome::Error(e) if e.code == ErrorCode::VersionMismatch));

    let status = HubAdmin::connect(&stack.socket()).unwrap().status().unwrap();

    assert_eq!(status.pid, stack.hub_pid());
    assert!(!status.on_demand, "this stack starts its hub by hand");
}

#[test]
fn a_client_can_tell_when_its_hub_does_not_know_a_request() {
    let stack = Stack::start();
    let client = stack.client();

    let refused = client.call(ClientRequest::Unknown).unwrap_err();

    assert!(refused.is_unsupported());
    client.hub_status().unwrap();
}

fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn only_this_user_can_reach_the_hub_or_read_its_data() {
    let stack = Stack::start();
    assert_eq!(mode(&stack.path("home")), 0o700);
    assert_eq!(mode(&stack.socket()), 0o600);
    assert_eq!(mode(&stack.path("home/hub.db")), 0o600);
    // Created once there's something to write, and they must not be any looser.
    let client = stack.client();
    stack.workspace(&client);
    assert_eq!(mode(&stack.path("home/hub.db-wal")), 0o600);
}

#[test]
fn a_directory_the_user_pointed_us_at_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let mut stack = Stack::start();
    stack.kill_hub();
    let shared = stack.path("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = stack.command();
    command.env("ROUNDHOUSE_HOME", &shared);
    let mut hub = command.spawn().unwrap();
    let socket = shared.join("hub.sock");
    wait_until("the hub", || Client::connect(&socket, "test").is_ok());
    assert_eq!(mode(&shared), 0o755);
    assert_eq!(mode(&socket), 0o600);
    assert_eq!(mode(&shared.join("hub.db")), 0o600);
    terminate(hub.id());
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
}

#[test]
fn dropping_a_client_really_disconnects_it() {
    let stack = Stack::start();
    let watcher = stack.client();
    let count = || watcher.hub_status().unwrap().clients;
    // The client `Stack::start` waits for the hub with may not have been seen to leave yet.
    wait_until("the hub to see the stack's own client leave", || count() == 1);
    let (_, session) = stack.workspace(&watcher);

    let other = stack.client();
    let _subscription = other.session_subscribe(&session.id, 1).unwrap();
    let clone = other.clone();
    assert_eq!(count(), 2);
    drop(other);
    assert_eq!(count(), 2, "a clone keeps it connected");
    drop(clone);
    wait_until("the hub to see the client leave", || count() == 1);
}

#[test]
fn dropping_a_subscription_stops_its_stream_but_not_the_connection() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let first = client.session_subscribe(&session.id, 1).unwrap();
    let id = first.id;
    drop(first);
    // The connection still works, and a new subscription gets everything from the start.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let second = client.session_subscribe(&session.id, 1).unwrap();
    assert_ne!(second.id, id);
    let mut follower = Follower::default();
    follower.drain_until(&second, "the whole log", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    assert_gapless(&follower.events);
}

#[test]
fn a_client_with_the_wrong_protocol_version_is_told_so() {
    use std::io::{BufRead, BufReader, Write};

    let stack = Stack::start();
    let mut stream = std::os::unix::net::UnixStream::connect(stack.socket()).unwrap();
    let hello = protocol::client::ClientFrame {
        id: 1,
        request: ClientRequest::Hello { protocol_version: 9999, client: "old".into() },
    };
    stream.write_all(&protocol::frame::encode(&hello).unwrap()).unwrap();
    let mut line = String::new();
    BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
    let reply: protocol::client::HubFrame = serde_json::from_str(&line).unwrap();
    match reply {
        protocol::client::HubFrame::Reply { outcome: protocol::client::ReplyOutcome::Error(e), .. } => {
            assert_eq!(e.code, ErrorCode::VersionMismatch);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_second_hub_refuses_to_start_and_leaves_the_first_alone() {
    let stack = Stack::start();
    let mut second = stack.command().spawn().unwrap();
    wait_until("the second hub to exit", || !matches!(second.try_wait(), Ok(None)));
    assert!(!second.wait().unwrap().success());
    assert_eq!(stack.client().hub_status().unwrap().pid, stack.hub_pid());
}

#[test]
fn losing_the_hub_ends_a_subscription_with_a_clear_signal() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let subscription = client.session_subscribe(&session.id, 1).unwrap();
    stack.kill_hub();
    let deadline = Instant::now() + TIMEOUT;
    while subscription.items.recv_timeout(deadline.saturating_duration_since(Instant::now())).is_ok() {}

    assert_eq!(subscription.ended(), Some(Ended::Disconnected));
    assert!(matches!(client.hub_status().unwrap_err(), ClientError::Disconnected | ClientError::Io(_)));
}

#[test]
fn a_kind_of_agent_or_side_of_a_diff_this_hub_doesnt_know_is_refused_and_not_saved() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);

    let refused =
        client.session_create(&workspace.id, AgentKind::Unknown, SessionSettingsChange::default(), None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    assert_eq!(client.session_list(&workspace.id).unwrap().len(), 1);

    let refused = client.comment_add(&workspace.id, "a.txt", DiffSide::Unknown, 1, "Why?", None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    assert!(client.comment_list(&workspace.id).unwrap().is_empty());
}
