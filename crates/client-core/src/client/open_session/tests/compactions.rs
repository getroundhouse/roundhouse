//! Following a session on a hub that answers the list of its compactions as a test says, for what a real hub can't be
//! made to do on cue.

use std::collections::VecDeque;
use std::path::Path;

use parking_lot::Mutex;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::event::CompactedBy;
use protocol::model::{CompactionLine, Place, Seq};

use super::super::{FollowError, OpenSession};
use super::{answered_simply, refused, requests_until_unsubscribed, session, start_hub_answering, until};
use crate::{Client, ClientError};

/// A hub that lists the session's prompts, and answers each ask for the list of compactions with the next of `lists`,
/// and anything else as [`answered_simply`], on each connection it's given. It reports every request it's sent.
fn start_hub(socket: &Path, lists: Vec<ReplyOutcome>) -> flume::Receiver<ClientRequest> {
    let lists = Mutex::new(VecDeque::from(lists));
    start_hub_answering(socket, move |request| {
        if matches!(request, ClientRequest::SessionCompactionList { .. }) {
            Some(lists.lock().pop_front().unwrap_or_else(|| {
                ReplyOutcome::Error(ApiError::new(ErrorCode::Internal, "asked for more lists than the test expects"))
            }))
        } else if matches!(request, ClientRequest::SessionPromptList { .. }) {
            Some(ReplyOutcome::Response(ClientResponse::Prompts { prompts: Vec::new() }))
        } else {
            answered_simply(request)
        }
    })
}

fn listed(lines: Vec<CompactionLine>) -> ReplyOutcome {
    ReplyOutcome::Response(ClientResponse::Compactions { compactions: lines })
}

fn far_back(seq: Seq) -> CompactionLine {
    CompactionLine {
        seq,
        place: Place::new(seq),
        by: CompactedBy::Agent,
        before_tokens: Some(467_499),
        after_tokens: Some(15_694),
    }
}

fn seqs(open: &OpenSession) -> Vec<Seq> {
    open.read(|log| log.compaction_lines().map(|line| line.seq).collect())
}

fn lists_asked(requests: &[ClientRequest]) -> usize {
    requests.iter().filter(|r| matches!(r, ClientRequest::SessionCompactionList { .. })).count()
}

#[test]
fn the_hubs_compactions_are_listed_once_its_subscription_has_started() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = start_hub(&socket, vec![listed(vec![far_back(3)])]);
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenSession::start(&client, &session());
    until("listed", || seqs(&open) == [3]);
    let errors = open.take_errors();
    drop(open);

    assert!(errors.is_empty(), "{errors:?}");
    let requests = requests_until_unsubscribed(&heard, 1);
    assert_eq!(lists_asked(&requests), 1, "{requests:?}");
    let subscribed = requests.iter().position(|r| matches!(r, ClientRequest::SessionSubscribe { .. }));
    let listed = requests.iter().position(|r| matches!(r, ClientRequest::SessionCompactionList { .. }));
    assert!(subscribed < listed, "it subscribes before it lists, so the list misses nothing: {requests:?}");
}

/// Such as one from before compactions were listed, but after prompts were.
#[test]
fn a_hub_too_old_to_list_compactions_is_followed_without_them_and_not_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = start_hub(&socket, vec![refused(ErrorCode::Unsupported, "unknown request")]);
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenSession::start(&client, &session());
    until("answered", || open.read(|log| !log.wants_compaction_list() && !log.wants_prompt_list()));
    open.want(0);
    let errors = open.take_errors();
    let connected = open.follow.connected();
    drop(open);

    assert!(errors.is_empty(), "{errors:?}");
    assert!(connected);
    assert_eq!(lists_asked(&requests_until_unsubscribed(&heard, 1)), 1);
}

#[test]
fn a_hub_that_fails_to_list_compactions_goes_on_being_followed_and_is_asked_again_on_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let lists = vec![refused(ErrorCode::Internal, "the disk is full"), listed(vec![far_back(3)])];
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
