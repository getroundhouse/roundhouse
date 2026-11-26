//! Following a session on a hub that answers the list of its prompts as a test says, for what a real hub can't be made
//! to do on cue.

use crate::Hub;
use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, log, subscriptions};
use protocol::model::{PromptLine, Seq, Session};

use super::super::{FollowError, OpenSession, View};
use super::{
    answered_simply, follows_session, refused, requests_until_unsubscribed, session, start_hub_answering, until,
};
use crate::stand_in_hub::refusing_every_start;
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
        } else if follows(request, GONE) {
            Some(refusing_every_start(request, ErrorCode::NotFound, "no such session"))
        } else {
            answered_simply(request)
        }
    })
}

/// Whether `request` starts following the session `id`.
fn follows(request: &ClientRequest, id: &str) -> bool {
    let ClientRequest::Subscribe(subscribe) = request else { return false };
    let follows = |start: &subscriptions::Subscription| matches!(&start.follow, subscriptions::Follow::Session { id: followed, .. } if followed.as_str() == id);
    subscribe.start.iter().any(follows)
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
fn the_hubs_prompts_are_listed_once_its_subscription_has_started() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = start_hub(&socket, vec![listed(vec![PromptLine::new(Seq::new(3), "far back", 0)])]);
    let client = Client::connect(&socket, "test").unwrap();

    let hub = Hub::caught_up_on(&client);
    let open = hub.open_session(&session(), View::Own);
    until("listed", || seqs(&open) == [Seq::new(3)]);
    let errors = open.take_errors();
    drop(open);

    assert!(errors.is_empty(), "{errors:?}");
    let requests = requests_until_unsubscribed(&heard, 1);
    assert_eq!(lists_asked(&requests), 1, "{requests:?}");
    let subscribed = requests.iter().position(|r| follows_session(r).is_some());
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
    let hub = Hub::caught_up_on(&client);
    let open = hub.open_session(&session(), View::Own);
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
    hub.carry_on_to(&again);
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

    let hub = Hub::caught_up_on(&client);
    let open = hub.open_session(&Session { id: GONE.into(), ..session() }, View::Own);

    open.follow.changed.recv_timeout(Duration::from_secs(10)).expect("never heard back");
    let mut requests = std::iter::from_fn(|| heard.recv_timeout(Duration::from_secs(10)).ok());
    assert!(requests.any(|r| follows_session(&r).is_some()), "it never asked");
    let errors = open.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!open.follow.connected());
}
