use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, log, subscriptions};
use protocol::model::{Place, Seq, Session};
use std::path::Path;
use std::time::Duration;

use crate::stand_in_hub::{Does, StandIn, stops};

/// Far enough along that the subscription starts after the first page, so a prompt there comes only from the list.
pub(in super::super) const LAST_SEQ: Seq = Seq::new(1800);

/// How far the agent's own events go by place, or a subagent's: a third of the way, as if they were every third event.
pub(in super::super) const LAST_PLACE: Place = Place::new(600);

/// The stand-in hub on `socket`, for each client that connects, answering each request but its hello as `answer` says,
/// if it says to. It reports every request it's sent, as it comes.
pub(in crate::client) fn start_hub_answering(
    socket: &Path,
    mut answer: impl FnMut(&ClientRequest) -> Option<ReplyOutcome> + Send + 'static,
) -> flume::Receiver<ClientRequest> {
    StandIn::of_build("scripted").to_every_connection().listen(socket, move |request| answers(answer(&request)))
}

/// Answers with `outcome`, or doesn't answer for `None`.
pub(in crate::client) fn answers(outcome: Option<ReplyOutcome>) -> Does {
    outcome.map_or(Does::Nothing, |outcome| Does::Answers(Box::new(outcome)))
}

/// What the hub answers that a test doesn't say: a read with no events, of a session that goes to [`LAST_SEQ`] by
/// `seq` and to [`LAST_PLACE`] by place, a start of following that it takes, sending nothing, nothing for letting go of
/// a subscription, which is sent without waiting for a reply, and Ok for anything else.
pub(in super::super) fn answered_simply(request: &ClientRequest) -> Option<ReplyOutcome> {
    if let ClientRequest::Subscribe(subscribe) = request {
        let subscribed = ClientResponse::Subscribed(subscriptions::Subscribed::default());
        (!subscribe.start.is_empty()).then_some(ReplyOutcome::Response(subscribed))
    } else if matches!(request, ClientRequest::SessionRead(_)) {
        Some(ReplyOutcome::Response(ClientResponse::Events(log::Events { events: Vec::new(), last_seq: LAST_SEQ })))
    } else if matches!(request, ClientRequest::SessionReadPlaces(_)) {
        let placed = ClientResponse::PlacedEvents(log::PlacedEvents { events: Vec::new(), last_place: LAST_PLACE });
        Some(ReplyOutcome::Response(placed))
    } else if matches!(request, ClientRequest::SessionCompactionList(_)) {
        Some(ReplyOutcome::Response(ClientResponse::Compactions(log::Compactions { compactions: Vec::new() })))
    } else {
        Some(ReplyOutcome::Response(ClientResponse::Ok))
    }
}

/// Where in the session's log `request` starts following it, if it starts following a session, which is from its
/// first `seq` when it doesn't say.
pub(in crate::client) fn follows_session(request: &ClientRequest) -> Option<subscriptions::LogFrom> {
    let ClientRequest::Subscribe(subscribe) = request else { return None };
    subscribe.start.iter().find_map(|start| {
        let subscriptions::Follow::Session { from, .. } = &start.follow else { return None };
        Some(from.clone().unwrap_or(subscriptions::LogFrom::Seq { seq: Seq::FIRST }))
    })
}

pub(in super::super) fn session() -> Session {
    Session { last_seq: LAST_SEQ, ..protocol::fixtures::session("s") }
}

pub(in crate::client) fn refused(code: ErrorCode, message: &str) -> ReplyOutcome {
    ReplyOutcome::Error(ApiError::new(code, message))
}

pub(in super::super) fn until(what: &str, done: impl FnMut() -> bool) {
    waiting::thread::until(Duration::from_secs(10), format!("never {what}"), done);
}

/// Every request the hub is sent until the follower's last subscription is let go, which is the last thing the
/// follower sends once every handle on it has gone.
pub(in super::super) fn requests_until_unsubscribed(
    heard: &flume::Receiver<ClientRequest>,
    subscriptions: usize,
) -> Vec<ClientRequest> {
    let mut requests = Vec::new();
    while requests.iter().filter(|r| stops(r)).count() < subscriptions {
        let Ok(request) = heard.recv_timeout(Duration::from_secs(10)) else {
            panic!("the follower never let go: {requests:?}");
        };
        requests.push(request);
    }
    requests
}
