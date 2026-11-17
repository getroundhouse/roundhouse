//! The connection to the hub.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;

use client_core::{Client, ClientError, Ended, HubAdmin};
use protocol::client::{ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome, Request};
use protocol::event::EventBody;
use protocol::model::Seq;

use crate::support::{Follower, Stack, TIMEOUT, assert_gapless, terminate, wait_for_turn_end, wait_until};

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

/// A request no hub knows, as from a client newer than it.
struct FromANewerClient;

impl Request for FromANewerClient {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::Unknown
    }
}

/// What a hub does with each of a connection's first frames is tested in the hub, in `crates/hub/src/server/tests.rs`.
/// These show the wiring, on a real hub's socket, each on a connection of its own. None changes the hub, so they share
/// one.
#[test]
fn a_real_hub_says_which_build_it_is_and_carries_on_past_what_it_refuses() {
    let stack = Stack::start();

    // The build: the daemon's, which was compiled with the same stamp as this test.
    let client = stack.client();
    let build = protocol::this_build!().version();
    assert_eq!(client.hub_version(), build, "the build, from the hello");
    assert_eq!(client.hub_status().unwrap().hub_version, build, "the build, from the status");

    // A request it doesn't know, which a client can tell, on a connection that carries on.
    let refused = client.call(FromANewerClient).unwrap_err();
    assert!(refused.is_unsupported(), "a request it doesn't know: {refused}");
    client.hub_status().expect("the connection to carry on past a request it doesn't know");

    // A hello it refuses, from a client that can still see the hub on a new connection.
    let mut refused = RawConnection::open(&stack);
    let hello = r#"{"method":"hello","client":"newer","protocol_version":9999}"#;
    let refusal = refused.ask(1, hello);
    assert!(matches!(&refusal, ReplyOutcome::Error(e) if e.code == ErrorCode::VersionMismatch), "a hello: {refusal:?}");
    let status = HubAdmin::connect(&stack.socket()).unwrap().status().unwrap();
    assert_eq!(status.pid, stack.hub_pid(), "the hub, after a refused hello");
    assert!(!status.on_demand, "this stack starts its hub by hand");
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
    let _subscription = other.session_subscribe(&session.id, Seq::new(1)).unwrap();
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
    let first = client.session_subscribe(&session.id, Seq::new(1)).unwrap();
    let id = first.id;
    drop(first);
    // The connection still works, and a new subscription gets everything from the start.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let second = client.session_subscribe(&session.id, Seq::new(1)).unwrap();
    assert_ne!(second.id, id);
    let mut follower = Follower::default();
    follower.drain_until(&second, "the whole log", |f| {
        f.events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. }))
    });
    assert_gapless(&follower.events);
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
    let subscription = client.session_subscribe(&session.id, Seq::new(1)).unwrap();
    stack.kill_hub();
    let deadline = Instant::now() + TIMEOUT;
    while subscription.items.recv_timeout(deadline.saturating_duration_since(Instant::now())).is_ok() {}

    assert_eq!(subscription.ended(), Some(Ended::Disconnected));
    assert!(client.is_disconnected());
    assert!(matches!(client.hub_status().unwrap_err(), ClientError::Disconnected | ClientError::Io(_)));
}
