//! Following a session on a hub that answers the list of its prompts as a test says, for what a real hub can't be made
//! to do on cue.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, log, subscriptions};
use protocol::model::{PromptLine, Seq, Session};

use super::super::{FollowError, OpenSession};
use super::{answered_simply, refused, requests_until_unsubscribed, session, start_hub_answering, until};
use crate::{Client, ClientError};

/// A session the hub has deleted, so it refuses to follow it.
const GONE: &str = "gone";

/// A hub that answers each ask for the list of prompts with the next of `lists`, following [`GONE`] with `NotFound`,
/// and anything else as [`answered_simply`], on each connection it's given. It reports every request it's sent.
fn start_hub(socket: &Path, lists: Vec<ReplyOutcome>) -> flume::Receiver<ClientRequest> {
    let lists = Mutex::new(VecDeque::from(lists));
    start_hub_answering(socket, move |request| {
        if matches!(request, ClientRequest::SessionPromptList(_)) {
            Some(lists.lock().pop_front().unwrap_or_else(|| {
                ReplyOutcome::Error(ApiError::new(ErrorCode::Internal, "asked for more lists than the test expects"))
            }))
        } else if matches!(request, ClientRequest::SessionSubscribe(subscriptions::SessionSubscribe { id, .. }) if id.as_str() == GONE)
        {
            Some(refused(ErrorCode::NotFound, "no such session"))
        } else {
            answered_simply(request)
        }
    })
}

fn listed(lines: Vec<PromptLine>) -> ReplyOutcome {
    ReplyOutcome::Response(ClientResponse::Prompts(log::Prompts { prompts: lines }))
}

fn seqs(open: &OpenSession) -> Vec<Seq> {
    open.read(|log| log.prompt_lines().map(PromptLine::seq).collect())
}

fn lists_asked(requests: &[ClientRequest]) -> usize {
    requests.iter().filter(|r| matches!(r, ClientRequest::SessionPromptList(_))).count()
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
    let subscribed = requests.iter().position(|r| matches!(r, ClientRequest::SessionSubscribe(_)));
    let listed = requests.iter().position(|r| matches!(r, ClientRequest::SessionPromptList(_)));
    assert!(subscribed < listed, "it subscribes before it lists, so the list misses nothing: {requests:?}");
}

#[test]
fn a_hub_that_fails_to_list_prompts_goes_on_being_followed_and_is_asked_again_on_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let lists = vec![
        refused(ErrorCode::Internal, "the disk is full"),
        listed(vec![PromptLine::new(Seq::new(3), "far back", 0)]),
    ];
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
    until("listed on the new connection", || seqs(&open) == [Seq::new(3)]);

    assert!(open.take_errors().is_empty());
    drop(open);
    let requests = requests_until_unsubscribed(&heard, 2);
    assert_eq!(lists_asked(&requests), 2, "once on each connection: {requests:?}");
}

/// Such as one another client deleted while this one was reconnecting.
#[test]
fn a_session_thats_been_deleted_is_let_go_of_rather_than_reported() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = start_hub(&socket, Vec::new());
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenSession::start(&client, &Session { id: GONE.into(), ..session() });

    open.follow.changed.recv_timeout(Duration::from_secs(10)).expect("never heard back");
    let mut requests = std::iter::from_fn(|| heard.recv_timeout(Duration::from_secs(10)).ok());
    assert!(requests.any(|r| matches!(r, ClientRequest::SessionSubscribe(_))), "it never asked");
    let errors = open.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!open.follow.connected());
}
