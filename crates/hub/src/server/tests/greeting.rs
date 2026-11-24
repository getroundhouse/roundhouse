//! What a hub does with the first frames on a connection from its own machine, over a stream in memory rather than
//! its socket.

use protocol::client::{
    CLIENT_PROTOCOL_VERSION, ClientResponse, ErrorCode, HUB_SPEAKS, HelloReply, HubFrame, ReplyOutcome,
};
use protocol::versions::Versions;

use super::connection::{Connection, call};
use crate::hub::tests::hub_in;

/// A hello from a client that speaks `newest`, and from `oldest` if it names a range.
fn hello(newest: impl std::fmt::Display, oldest: Option<u32>) -> String {
    let oldest = oldest.map_or_default(|oldest| format!(r#","oldest_protocol_version":{oldest}"#));
    format!(r#"{{"method":"hello","client":"test","protocol_version":{newest}{oldest}}}"#)
}

const STATUS: &str = r#"{"method":"hub_status"}"#;

/// What the hub does with a frame.
enum Then {
    /// Takes a hello, at this version.
    Greets(u32),
    /// Says its status.
    ShowsStatus,
    /// Takes it as a request, whatever it answers, rather than refusing it for want of a hello.
    Handles,
    /// Refuses it with this code, saying this in its message.
    Refuses(ErrorCode, String),
    /// Closes the connection without a reply.
    Closes,
}

impl Then {
    fn holds(&self, reply: Option<&HubFrame>, id: u64) -> bool {
        let Some(HubFrame::Reply { id: answered, outcome }) = reply else {
            return matches!((self, reply), (Then::Closes, None));
        };
        *answered == id
            && match (self, outcome) {
                (
                    Then::Greets(version),
                    ReplyOutcome::Response(ClientResponse::Hello(HelloReply { protocol_version, .. })),
                ) => protocol_version == version,
                (Then::ShowsStatus, ReplyOutcome::Response(ClientResponse::HubStatus(status))) => {
                    let speaks = Versions::from_hello(status.protocol_version, status.oldest_protocol_version);
                    status.pid == std::process::id() && speaks == HUB_SPEAKS
                }
                (Then::Handles, ReplyOutcome::Response(_)) => true,
                (Then::Handles, ReplyOutcome::Error(error)) => error.code != ErrorCode::VersionMismatch,
                (Then::Refuses(code, says), ReplyOutcome::Error(error)) => {
                    error.code == *code && error.message.contains(says.as_str())
                }
                _ => false,
            }
    }
}

/// One step of a case: the line sent, the call it makes, and what the hub does with it.
type Step = (String, u64, Then);

/// Each of `requests` as a call, numbered from 1, and what the hub does with it.
fn calls(requests: Vec<(&str, Then)>) -> Vec<Step> {
    (1..).zip(requests).map(|(id, (request, then))| (call(id, request), id, then)).collect()
}

/// What a connection sends first, and what the hub does with each frame. A refusal that closes the connection is
/// followed by a request for the hub's status, which goes unanswered.
fn cases() -> Vec<(&'static str, Vec<Step>)> {
    let (ours, newest) = (CLIENT_PROTOCOL_VERSION, HUB_SPEAKS.newest);
    let (hello_ours, hello_oldest, newer_range) =
        (hello(ours, None), hello(HUB_SPEAKS.oldest, None), hello(newest + 1, Some(ours)));
    let (past_range, unspoken, unreadable) =
        (hello(newest + 2, Some(newest + 1)), hello(9999, None), hello(r#""3""#, None));
    let mismatch = |says: &str| Then::Refuses(ErrorCode::VersionMismatch, says.to_string());
    let past = mismatch(&format!("protocols {} to {}", newest + 1, newest + 2));
    let unsupported = || Then::Refuses(ErrorCode::Unsupported, String::new());
    let repos = r#"{"method":"repo_list"}"#;
    // As a request this hub doesn't know: here, the answer a client sent before decisions were typed.
    let approve = r#"{"method":"session_approve","id":"s","approval_id":"a","decision":{"decision":"allow"}}"#;
    vec![
        ("a hello at this version", calls(vec![(&hello_ours, Then::Greets(ours)), (STATUS, Then::ShowsStatus)])),
        (
            "a hello at the oldest version this hub speaks",
            calls(vec![(&hello_oldest, Then::Greets(HUB_SPEAKS.oldest)), (STATUS, Then::ShowsStatus)]),
        ),
        (
            "a hello naming a range that reaches past the newest version this hub speaks",
            calls(vec![(&newer_range, Then::Greets(newest)), (STATUS, Then::ShowsStatus)]),
        ),
        (
            "a hello naming a range that starts past this version",
            calls(vec![(&past_range, past), (STATUS, Then::Closes)]),
        ),
        ("a hello at a version this hub doesn't speak", calls(vec![(&unspoken, mismatch("")), (STATUS, Then::Closes)])),
        // One it can't read can't be a hello it answers, so it's refused as any first request but a hello is.
        ("a hello this hub can't read", calls(vec![(&unreadable, mismatch("")), (STATUS, Then::Closes)])),
        ("a request before a hello", calls(vec![(repos, mismatch("")), (STATUS, Then::Closes)])),
        // A client across a real break sends these as written, so they're spelled out rather than built from today's
        // types. Seeing the status isn't saying hello, so nothing else works after it.
        ("the hub's status without a hello", calls(vec![(STATUS, Then::ShowsStatus), (repos, mismatch(""))])),
        (
            "a request this hub doesn't know",
            calls(vec![
                (&hello_ours, Then::Greets(ours)),
                (r#"{"method":"from_a_newer_client","path":"a"}"#, unsupported()),
                (STATUS, Then::ShowsStatus),
            ]),
        ),
        (
            "a request this hub can't read",
            calls(vec![(&hello_ours, Then::Greets(ours)), (approve, unsupported()), (STATUS, Then::ShowsStatus)]),
        ),
        ("a frame naming no call", vec![(r#"{"request":{"method":"hub_status"}}"#.to_string(), 1, Then::Closes)]),
        // Last, since they leave the hub restarting and then stopping, which in a test's process stops nothing. A
        // restart is refused where it would stop the hub's agents, which is still answering it.
        (
            "restarting or stopping the hub without a hello",
            calls(vec![
                (r#"{"method":"hub_restart"}"#, Then::Handles),
                (r#"{"method":"hub_shutdown"}"#, Then::Handles),
                (repos, mismatch("")),
            ]),
        ),
    ]
}

/// Each case is a connection of its own, which leaves the hub as it found it, so every case shares one hub.
#[tokio::test]
async fn a_hub_greets_refuses_or_closes_a_connection_by_its_first_frames() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    for (case, steps) in cases() {
        let mut connection = Connection::open(&hub);
        for (line, id, then) in steps {
            let reply = connection.send(&line, case).await;

            assert!(then.holds(reply.as_ref(), id), "{case}, call {id}: {reply:?}");
        }
    }
}

/// A client on the hub's machine says in its hello which device it's on, which a request that needs one reads. One
/// that says none, or one the hub can't show, is greeted all the same, and refused what needs a device.
#[tokio::test]
async fn a_request_that_needs_a_device_takes_the_one_its_hello_said() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let ours = CLIENT_PROTOCOL_VERSION;
    let on =
        |device: &str| format!(r#"{{"method":"hello","client":"test","protocol_version":{ours},"device":{device}}}"#);
    let start = r#"{"method":"session_conversation_start","id":"no-such-chat"}"#;
    let no_device = || Then::Refuses(ErrorCode::InvalidRequest, "which device".to_string());
    // Past the device, to the chat, which isn't there.
    let on_one = Then::Refuses(ErrorCode::NotFound, String::new());
    let cases = [
        ("no device", hello(ours, None), no_device()),
        ("a device", on(r#"{"id":"d1","name":"Mac Studio"}"#), on_one),
        ("a device whose name can't be shown", on(r#"{"id":"d1","name":"Mac\u0007Studio"}"#), no_device()),
        ("a device with no ID", on(r#"{"id":" ","name":"Mac Studio"}"#), no_device()),
    ];

    for (case, hello, then) in cases {
        let mut connection = Connection::open(&hub);
        for (line, id, then) in calls(vec![(&hello, Then::Greets(ours)), (start, then)]) {
            let reply = connection.send(&line, case).await;

            assert!(then.holds(reply.as_ref(), id), "{case}, call {id}: {reply:?}");
        }
    }
}
