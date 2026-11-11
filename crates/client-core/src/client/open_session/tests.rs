//! Following a session on a hub that answers the list of its prompts as a test says, for what a real hub can't be made
//! to do on cue.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::frame;
use protocol::model::PromptLine;

use super::*;
use crate::ClientError;

/// Far enough along that the subscription starts after the first page, so a prompt there comes only from the list.
const LAST_SEQ: Seq = 600;

/// A hub that answers each ask for the list of prompts with the next of `lists`, a read with no events, and anything
/// else with Ok, on each connection it's given. It reports every request it's sent.
fn start_hub(socket: &Path, lists: Vec<ReplyOutcome>) -> flume::Receiver<ClientRequest> {
    let listener = UnixListener::bind(socket).unwrap();
    let lists = Arc::new(Mutex::new(VecDeque::from(lists)));
    let (asked, heard) = flume::unbounded();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let (stream, lists, asked) = (stream.unwrap(), lists.clone(), asked.clone());
            std::thread::spawn(move || serve(stream, &lists, &asked));
        }
    });
    heard
}

fn serve(mut stream: UnixStream, lists: &Mutex<VecDeque<ReplyOutcome>>, asked: &flume::Sender<ClientRequest>) {
    for line in BufReader::new(stream.try_clone().unwrap()).lines() {
        let Ok(line) = line else { return };
        let ClientFrame { id, request } = frame::decode(line.as_bytes()).unwrap();
        let response = |response| Some(ReplyOutcome::Response(response));
        let outcome = if matches!(request, ClientRequest::Hello { .. }) {
            response(ClientResponse::Hello { protocol_version: PROTOCOL_VERSION, hub_version: "scripted".into() })
        } else if matches!(request, ClientRequest::SessionPromptList { .. }) {
            Some(lists.lock().pop_front().unwrap_or_else(|| {
                ReplyOutcome::Error(ApiError::new(ErrorCode::Internal, "asked for more lists than the test expects"))
            }))
        } else if matches!(request, ClientRequest::SessionRead { .. }) {
            response(ClientResponse::Events { events: Vec::new(), last_seq: LAST_SEQ })
        } else if matches!(request, ClientRequest::SessionUnsubscribe { .. }) {
            // Sent without waiting for a reply.
            None
        } else {
            response(ClientResponse::Ok)
        };
        if let Some(outcome) = outcome {
            stream.write_all(&frame::encode(&HubFrame::Reply { id, outcome }).unwrap()).unwrap();
        }
        let _ = asked.send(request);
    }
}

fn session() -> Session {
    let record = format!(
        r#"{{"id":"s","workspace_id":"w","agent":"claude","model":"opus","permission_mode":null,"effort":"xhigh","status":"idle",
        "last_seq":{LAST_SEQ},"created_at_ms":0,"updated_at_ms":0}}"#
    );
    serde_json::from_str(&record).unwrap()
}

fn listed(lines: Vec<PromptLine>) -> ReplyOutcome {
    ReplyOutcome::Response(ClientResponse::Prompts { prompts: lines })
}

fn refused(code: ErrorCode, message: &str) -> ReplyOutcome {
    ReplyOutcome::Error(ApiError::new(code, message))
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "never {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn seqs(open: &OpenSession) -> Vec<Seq> {
    open.read(|log| log.prompt_lines().map(PromptLine::seq).collect())
}

/// Every request the hub is sent until the follower's last subscription is let go, which is the last thing the
/// follower sends once every handle on it has gone.
fn requests_until_unsubscribed(heard: &flume::Receiver<ClientRequest>, subscriptions: usize) -> Vec<ClientRequest> {
    let mut requests = Vec::new();
    while requests.iter().filter(|r| matches!(r, ClientRequest::SessionUnsubscribe { .. })).count() < subscriptions {
        let Ok(request) = heard.recv_timeout(Duration::from_secs(10)) else {
            panic!("the follower never let go: {requests:?}");
        };
        requests.push(request);
    }
    requests
}

fn lists_asked(requests: &[ClientRequest]) -> usize {
    requests.iter().filter(|r| matches!(r, ClientRequest::SessionPromptList { .. })).count()
}

#[test]
fn a_hub_too_old_to_list_prompts_is_followed_without_them_and_not_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = start_hub(&socket, vec![refused(ErrorCode::Unsupported, "unknown request")]);
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenSession::start(&client, &session());
    until("answered", || open.read(|log| !log.wants_prompt_list()));
    open.want(0);
    let errors = open.take_errors();
    let connected = open.follow.connected();
    drop(open);

    assert!(errors.is_empty(), "{errors:?}");
    assert!(connected);
    let requests = requests_until_unsubscribed(&heard, 1);
    assert_eq!(lists_asked(&requests), 1, "{requests:?}");
    let subscribed = requests.iter().position(|r| matches!(r, ClientRequest::SessionSubscribe { .. }));
    let listed = requests.iter().position(|r| matches!(r, ClientRequest::SessionPromptList { .. }));
    assert!(subscribed < listed, "it subscribes before it lists, so the list misses nothing: {requests:?}");
}

#[test]
fn a_hub_that_fails_to_list_prompts_goes_on_being_followed_and_is_asked_again_on_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let lists = vec![refused(ErrorCode::Internal, "the disk is full"), listed(vec![PromptLine::new(3, "far back", 0)])];
    let heard = start_hub(&socket, lists);
    let client = Client::connect(&socket, "test").unwrap();
    let open = OpenSession::start(&client, &session());
    let mut errors = Vec::new();

    until("told of the failure", || {
        errors.extend(open.take_errors());
        !errors.is_empty()
    });

    assert!(open.follow.connected(), "the chat goes on without the list");
    assert!(
        matches!(&errors[..], [FollowError::Asked(ClientError::Api(e))] if e.message == "the disk is full"),
        "{errors:?}"
    );
    assert!(seqs(&open).is_empty());

    // Kept here too, so the connection is still open for the follower to let go of its subscription on.
    let again = Client::connect(&socket, "test").unwrap();
    open.reconnect(&again);
    until("listed on the new connection", || seqs(&open) == [3]);

    assert!(open.take_errors().is_empty());
    drop(open);
    let requests = requests_until_unsubscribed(&heard, 2);
    assert_eq!(lists_asked(&requests), 2, "once on each connection: {requests:?}");
}
