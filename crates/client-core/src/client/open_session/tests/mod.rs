//! Following a session on hubs scripted for what a real hub can't be made to do on cue.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::client::{ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome, log};
use protocol::frame;
use protocol::model::{Place, Seq, Session};

mod compactions;
mod prompts;
mod views;
mod waiting;

/// Far enough along that the subscription starts after the first page, so a prompt there comes only from the list.
pub(super) const LAST_SEQ: Seq = Seq::new(1800);

/// How far the agent's own events go by place, or a subagent's: a third of the way, as if they were every third event.
pub(super) const LAST_PLACE: Place = Place::new(600);

/// A hub that answers each request but its hello as `answer` says, if it says to, on each connection it's given. Each
/// is answered on a thread of its own, so an answer can wait without holding up the rest. It reports every request
/// it's sent, as it comes.
pub(in crate::client) fn start_hub_answering(
    socket: &Path,
    answer: impl Fn(&ClientRequest) -> Option<ReplyOutcome> + Send + Sync + 'static,
) -> flume::Receiver<ClientRequest> {
    let listener = UnixListener::bind(socket).unwrap();
    let answer: Arc<Answer> = Arc::new(answer);
    let (asked, heard) = flume::unbounded();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let (stream, answer, asked) = (stream.unwrap(), answer.clone(), asked.clone());
            std::thread::spawn(move || serve(stream, &answer, &asked));
        }
    });
    heard
}

type Answer = dyn Fn(&ClientRequest) -> Option<ReplyOutcome> + Send + Sync;

fn serve(stream: UnixStream, answer: &Arc<Answer>, asked: &flume::Sender<ClientRequest>) {
    let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let ClientFrame { id, request } = frame::decode(line.as_bytes()).unwrap();
        let _ = asked.send(request.clone());
        let (writer, answer) = (writer.clone(), answer.clone());
        std::thread::spawn(move || {
            let outcome = if matches!(request, ClientRequest::Hello(_)) {
                Some(ReplyOutcome::Response(crate::stand_in_hub::hello_reply("scripted")))
            } else {
                answer(&request)
            };
            if let Some(outcome) = outcome {
                // Gone once the test is.
                let _ = writer.lock().write_all(&frame::encode(&HubFrame::Reply { id, outcome }).unwrap());
            }
        });
    }
}

/// What the hub answers that a test doesn't say: a read with no events, of a session that goes to [`LAST_SEQ`] by
/// `seq` and to [`LAST_PLACE`] by place, nothing for letting go of a subscription, which is sent without waiting for a
/// reply, and Ok for anything else.
pub(super) fn answered_simply(request: &ClientRequest) -> Option<ReplyOutcome> {
    if matches!(request, ClientRequest::SessionRead(_)) {
        Some(ReplyOutcome::Response(ClientResponse::Events(log::Events { events: Vec::new(), last_seq: LAST_SEQ })))
    } else if matches!(request, ClientRequest::SessionReadPlaces(_)) {
        let placed = ClientResponse::PlacedEvents(log::PlacedEvents { events: Vec::new(), last_place: LAST_PLACE });
        Some(ReplyOutcome::Response(placed))
    } else if matches!(request, ClientRequest::SessionCompactionList(_)) {
        Some(ReplyOutcome::Response(ClientResponse::Compactions(log::Compactions { compactions: Vec::new() })))
    } else if matches!(request, ClientRequest::SessionUnsubscribe(_)) {
        None
    } else {
        Some(ReplyOutcome::Response(ClientResponse::Ok))
    }
}

pub(super) fn session() -> Session {
    Session { last_seq: LAST_SEQ, ..protocol::fixtures::session("s") }
}

pub(in crate::client) fn refused(code: ErrorCode, message: &str) -> ReplyOutcome {
    ReplyOutcome::Error(ApiError::new(code, message))
}

pub(super) fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "never {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every request the hub is sent until the follower's last subscription is let go, which is the last thing the
/// follower sends once every handle on it has gone.
pub(super) fn requests_until_unsubscribed(
    heard: &flume::Receiver<ClientRequest>,
    subscriptions: usize,
) -> Vec<ClientRequest> {
    let mut requests = Vec::new();
    while requests.iter().filter(|r| matches!(r, ClientRequest::SessionUnsubscribe(_))).count() < subscriptions {
        let Ok(request) = heard.recv_timeout(Duration::from_secs(10)) else {
            panic!("the follower never let go: {requests:?}");
        };
        requests.push(request);
    }
    requests
}
