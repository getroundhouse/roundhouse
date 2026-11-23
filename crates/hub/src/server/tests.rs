//! What a hub does with the first frames on a connection from its own machine, over a stream in memory rather than
//! its socket.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{
    CLIENT_PROTOCOL_VERSION, ClientRequest, ClientResponse, ErrorCode, HUB_SPEAKS, Hello, HelloReply, HubFrame,
    ReplyOutcome,
};
use protocol::frame;
use protocol::versions::Versions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::time::timeout;

use super::admit::Gate;
use super::connection::serve_connection;
use crate::hub::Hub;
use crate::hub::tests::hub_in;

/// Long enough for anything here on a busy machine.
pub(super) const WAIT: Duration = Duration::from_secs(10);

/// A connection to `hub`, served as one on its socket is.
pub(super) struct Connection {
    replies: Lines<BufReader<ReadHalf<DuplexStream>>>,
    frames: WriteHalf<DuplexStream>,
    /// What the hub has sent but replies, in order, as [`Connection::reply`] and [`Connection::until`] read it.
    pub(super) sent: Vec<HubFrame>,
}

impl Connection {
    pub(super) fn open(hub: &Arc<Hub>) -> Self {
        let (client, served) = tokio::io::duplex(1 << 16);
        let (read, write) = tokio::io::split(served);
        tokio::spawn(serve_connection(hub.clone(), read, write, Gate::local(hub)));
        let (replies, frames) = tokio::io::split(client);
        Self { replies: BufReader::new(replies).lines(), frames, sent: Vec::new() }
    }

    /// One that has said hello to `hub` at version `version` of the client protocol.
    pub(super) async fn greeted(hub: &Arc<Hub>, version: u32) -> Self {
        let mut connection = Self::open(hub);
        let hello = ClientRequest::Hello(Hello {
            protocol_version: version,
            client: "test".into(),
            oldest_protocol_version: None,
            device: None,
        });
        let greeted = connection.reply(0, &hello).await;
        assert!(matches!(greeted, ReplyOutcome::Response(ClientResponse::Hello(_))), "{greeted:?}");
        connection
    }

    /// Sends `line`, for `case`, and gives back the next frame, or `None` once the hub has closed the connection.
    async fn send(&mut self, line: &str, case: &str) -> Option<HubFrame> {
        // A hub that has closed the connection may no longer take it, which what it sends back says.
        let _ = self.frames.write_all(format!("{line}\n").as_bytes()).await;
        let next = timeout(WAIT, self.replies.next_line()).await;
        let next =
            next.unwrap_or_else(|_| panic!("{case}: the hub neither replied nor closed the connection")).unwrap();
        next.map(|line| frame::decode(line.as_bytes()).unwrap())
    }

    /// Sends `request` as call `id`, and gives back its reply, keeping in `sent` whatever else the hub sends first.
    pub(super) async fn reply(&mut self, id: u64, request: &ClientRequest) -> ReplyOutcome {
        self.write(id, request).await;
        self.reply_to(id).await
    }

    /// Sends `request` as call `id`, without waiting for its reply.
    pub(super) async fn write(&mut self, id: u64, request: &ClientRequest) {
        let line = call(id, &serde_json::to_string(request).unwrap());
        self.frames.write_all(format!("{line}\n").as_bytes()).await.unwrap();
    }

    /// The reply to call `id`, keeping in `sent` whatever else the hub sends first.
    pub(super) async fn reply_to(&mut self, id: u64) -> ReplyOutcome {
        loop {
            match self.next().await {
                Some(HubFrame::Reply { id: answered, outcome }) if answered == id => return outcome,
                Some(frame) => self.sent.push(frame),
                None => panic!("the hub closed the connection"),
            }
        }
    }

    /// Reads what the hub sends into `sent` until it holds `done`.
    pub(super) async fn until(&mut self, what: &str, done: impl Fn(&[HubFrame]) -> bool) {
        while !done(&self.sent) {
            match self.next().await {
                Some(frame) => self.sent.push(frame),
                None => panic!("the hub closed the connection waiting for {what}, having sent {:#?}", self.sent),
            }
        }
    }

    /// The next frame the hub sends, or `None` once it has closed the connection.
    async fn next(&mut self) -> Option<HubFrame> {
        let line = timeout(WAIT, self.replies.next_line()).await;
        let line = line.unwrap_or_else(|_| panic!("the hub sent nothing more, having sent {:#?}", self.sent)).unwrap();
        line.map(|line| frame::decode(line.as_bytes()).unwrap())
    }
}

/// A call of `request`, a request's JSON, as `id`.
fn call(id: u64, request: &str) -> String {
    format!(r#"{{"id":{id},"request":{request}}}"#)
}

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
