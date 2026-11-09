//! The connection to the hub.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;

use client_core::{Client, ClientError, HubAdmin, Subscription, SubscriptionItem};
use protocol::client::{ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, ReplyOutcome};
use protocol::event::EventBody;
use protocol::model::{AgentKind, DiffSide, RecordKey, SessionStatus, WorkspaceStatus};

use crate::support::{
    Follower, Stack, TIMEOUT, assert_gapless, terminate, wait_for_setup, wait_for_turn_end, wait_until,
};

fn next_notice(feed: &Subscription, what: &str) -> HubNotice {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match feed.items.recv_timeout(wait) {
            Ok(SubscriptionItem::Notice(notice)) => return notice,
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for {what}: {e}"),
        }
    }
}

#[test]
fn the_feed_reports_what_a_sidebar_shows() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();

    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    assert_eq!(next_notice(&feed, "the repo"), HubNotice::Repo(repo.clone()));
    let workspace = client.workspace_create(&repo.id, Some("watched".into()), None, None).unwrap();
    // Ready, then its setup running and finishing.
    let HubNotice::Workspace(ready) = next_notice(&feed, "the workspace") else { panic!() };
    assert_eq!(ready.id, workspace.id);
    wait_for_setup(&client, &workspace.id);
    for _ in 0..2 {
        assert!(matches!(next_notice(&feed, "setup"), HubNotice::Workspace(_)));
    }
    let session =
        client.session_create(&workspace.id, AgentKind::Claude, None, Some("bypassPermissions".into()), None).unwrap();
    assert_eq!(next_notice(&feed, "the session"), HubNotice::Session(session.clone()));

    // One notice per change of status, not one per event.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    let HubNotice::Session(running) = next_notice(&feed, "running") else { panic!() };
    assert_eq!((running.id.as_str(), running.status), (session.id.as_str(), SessionStatus::Running));
    let HubNotice::Session(idle) = next_notice(&feed, "idle") else { panic!() };
    assert_eq!(idle.status, SessionStatus::Idle);
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);

    let archived = client.workspace_archive(&workspace.id, true).unwrap();
    assert_eq!(next_notice(&feed, "the archive"), HubNotice::Workspace(archived));
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let HubNotice::Workspace(seen) = next_notice(&feed, "the restore") else { panic!() };
    assert_eq!((seen.id.as_str(), seen.status), (restored.id.as_str(), WorkspaceStatus::Ready));
}

#[test]
fn a_list_and_the_feed_agree_on_which_copy_is_newer() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();
    let (workspace, session) = stack.workspace(&client);
    let before = client.records_list().unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let comment = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this", None).unwrap();
    client.comment_delete(&comment.id).unwrap();
    let mut notices = Vec::new();
    while !matches!(notices.last(), Some(HubNotice::Removed { .. })) {
        notices.push(next_notice(&feed, "the comment's removal"));
    }
    let after = client.records_list().unwrap();

    // Each change since the first list is newer than it, and the second list is at least as new.
    let newest_session = notices.iter().rev().find_map(|n| match n {
        HubNotice::Session(s) if s.id == session.id && s.rev > before.rev => Some(s),
        _ => None,
    });
    let listed = after.sessions.iter().find(|s| s.id == session.id).unwrap();
    assert!(listed.rev >= newest_session.unwrap().rev);
    assert_eq!(listed.status, SessionStatus::Idle);
    let added = notices.iter().find_map(|n| match n {
        HubNotice::Comment(c) if c.id == comment.id => Some(c),
        _ => None,
    });
    assert_eq!(added, Some(&comment));
    let Some(HubNotice::Removed { record, rev }) = notices.last() else { unreachable!() };
    assert_eq!(record, &RecordKey::Comment { id: comment.id.clone() });
    assert!(before.rev < comment.rev && comment.rev < *rev && *rev <= after.rev);
    assert!(after.comments.is_empty());
}

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
        writeln!(self.socket, r#"{{"id":{id},"request":{request}}}"#).unwrap();
        let mut line = String::new();
        self.replies.read_line(&mut line).unwrap();
        match protocol::frame::decode(line.as_bytes()).unwrap() {
            HubFrame::Reply { id: answered, outcome } if answered == id => outcome,
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
    let stopped = connection.ask(2, r#"{"method":"hub_shutdown"}"#);
    assert_eq!(stopped, ReplyOutcome::Response(ClientResponse::Ok));
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
    assert_eq!(count(), 1);
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
    loop {
        let item = subscription.items.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap();
        if item == SubscriptionItem::Disconnected {
            break;
        }
    }
    assert!(matches!(client.hub_status().unwrap_err(), ClientError::Disconnected | ClientError::Io(_)));
}

#[test]
fn a_kind_of_agent_or_side_of_a_diff_this_hub_doesnt_know_is_refused_and_not_saved() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);

    let refused = client.session_create(&workspace.id, AgentKind::Unknown, None, None, None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    assert_eq!(client.session_list(&workspace.id).unwrap().len(), 1);

    let refused = client.comment_add(&workspace.id, "a.txt", DiffSide::Unknown, 1, "Why?", None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    assert!(client.comment_list(&workspace.id).unwrap().is_empty());
}
