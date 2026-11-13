//! Searching a session on a hub that answers as a test says.

use std::time::Duration;

use protocol::client::{ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, log};
use protocol::event::{EventBody, NoticeLevel};
use protocol::find::Found;
use protocol::model::{Place, Seq, ToolUseId};

use super::super::OpenSession;
use super::super::tests::{
    LAST_PLACE, answered_simply, refused, requests_until_unsubscribed, session, start_hub_answering, until,
};
use super::*;
use crate::session_log::{By, SessionLog};

/// The position of the event at `place`, in a log by place, as a search reads.
fn at(place: u64) -> Position {
    Position::of_place(Place::new(place))
}

fn at_each(places: &[u64]) -> Vec<Position> {
    places.iter().copied().map(at).collect()
}

/// Long enough for a search that's to be held up to have asked for what it's not to.
const A_WHILE: Duration = Duration::from_millis(100);

fn needle(query: &str) -> Needle {
    Needle::new(query).unwrap()
}

fn found(places: &[u64]) -> ReplyOutcome {
    let places = places.iter().copied().map(Place::new).collect();
    ReplyOutcome::Response(ClientResponse::Found(Found { places, through: LAST_PLACE }))
}

/// The events of the agent or subagent asked for from a read by place, each a notice, to its end, each the third event
/// after the one before, so its `seq` is far from its place, as a subagent's is.
fn read(request: &ClientRequest) -> Option<ReplyOutcome> {
    let ClientRequest::SessionReadPlaces(log::SessionReadPlaces { subagent, from_place, limit, .. }) = request else {
        return answered_simply(request);
    };
    let from = from_place.get();
    let events = (from..(from + u64::from(*limit)).min(LAST_PLACE.get() + 1))
        .map(|place| {
            let body =
                EventBody::Notice { level: NoticeLevel::Info, message: format!("{place}"), parent_tool_use_id: None };
            SessionEvent {
                subagent: subagent.clone(),
                place: Place::new(place),
                ..protocol::fixtures::event(Seq::new(place * 3), body)
            }
        })
        .collect();
    Some(ReplyOutcome::Response(ClientResponse::PlacedEvents(log::PlacedEvents { events, last_place: LAST_PLACE })))
}

/// Where each read the hub has been sent for a stretch so far starts, and whose events it reads, gathering them
/// into `heard`. The first read, of one event from the first place, asks how far the events go.
fn reads_from(
    requests: &flume::Receiver<ClientRequest>,
    heard: &mut Vec<ClientRequest>,
) -> Vec<(u64, Option<ToolUseId>)> {
    heard.extend(requests.try_iter());
    heard
        .iter()
        .filter_map(|request| {
            let ClientRequest::SessionReadPlaces(log::SessionReadPlaces { from_place, limit, subagent, .. }) = request
            else {
                return None;
            };
            (*limit > 1).then(|| (from_place.get(), subagent.clone()))
        })
        .collect()
}

fn queries_asked(requests: &[ClientRequest]) -> Vec<(&str, Option<&ToolUseId>)> {
    requests
        .iter()
        .filter_map(|request| {
            let ClientRequest::SessionFind(log::SessionFind { query, subagent, .. }) = request else { return None };
            Some((query.as_str(), subagent.as_ref()))
        })
        .collect()
}

fn connect(socket: &std::path::Path) -> Client {
    Client::connect(socket, "test").unwrap()
}

/// The chat's own events, followed by place, as the app follows them.
fn own(client: &Client) -> OpenSession {
    OpenSession::start_view(client, &session(), View::Own)
}

#[test]
fn what_the_hub_names_is_read_with_what_s_around_it_a_stretch_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let requests = start_hub_answering(&socket, |request| {
        if matches!(request, ClientRequest::SessionFind(_)) { Some(found(&[100, 300, 500])) } else { read(request) }
    });
    let client = connect(&socket);
    let helper: ToolUseId = "helper".into();
    let open = OpenSession::start_view(&client, &session(), View::Subagent(helper.clone()));
    let mut heard = Vec::new();
    let starts = |reads: Vec<(u64, Option<ToolUseId>)>| -> Vec<u64> {
        assert!(reads.iter().all(|(_, of)| of.as_ref() == Some(&helper)), "{reads:?}");
        reads.into_iter().map(|(from, _)| from).collect()
    };

    let search = open.search(&client, needle("parser"), 64);
    until("two stretches are read", || reads_from(&requests, &mut heard).len() == 2);
    std::thread::sleep(A_WHILE);

    assert_eq!(starts(reads_from(&requests, &mut heard)), [36, 236], "not the third, with two waiting");
    assert_eq!(queries_asked(&heard), [("parser", Some(&helper))], "of the subagent the view follows");
    assert_eq!(search.searched(), Searched::Underway);
    let first = search.take().unwrap();
    assert_eq!(first.holding, [at(100)]);
    let places: Vec<u64> = first.events.iter().map(|event| event.place.get()).collect();
    assert_eq!(places, (36..165).collect::<Vec<_>>(), "everything within 64 places of it");
    until("the third is read", || reads_from(&requests, &mut heard).len() == 3);
    assert_eq!(search.take().unwrap().holding, [at(300)]);
    until("the last is sent", || search.stretches.len() == 1);
    assert_eq!(search.searched(), Searched::Underway, "it isn't done while the last is still to be taken");
    assert_eq!(search.take().unwrap().holding, [at(500)]);
    until("done", || search.searched() == Searched::Done);
    assert!(search.take().is_none());
}

#[test]
fn a_chat_has_one_search_at_the_hub_at_a_time_and_one_typed_over_isn_t_asked() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (answer, answered) = flume::unbounded::<()>();
    let requests = start_hub_answering(&socket, move |request| {
        if matches!(request, ClientRequest::SessionFind(_)) {
            answered.recv().unwrap();
            Some(found(&[]))
        } else {
            answered_simply(request)
        }
    });
    let client = connect(&socket);
    let open = own(&client);
    let mut heard = Vec::new();
    let mut finds = || {
        heard.extend(requests.try_iter());
        queries_asked(&heard).len()
    };

    let first = open.search(&client, needle("pa"), 64);
    until("the first is asked", || finds() == 1);
    drop(open.search(&client, needle("par"), 64));
    let third = open.search(&client, needle("pars"), 64);
    std::thread::sleep(A_WHILE);

    assert_eq!(finds(), 1, "the others wait while the first is at the hub");
    answer.send(()).unwrap();
    answer.send(()).unwrap();
    until("both are done", || first.searched() == Searched::Done && third.searched() == Searched::Done);
    drop(open);
    heard.extend(requests_until_unsubscribed(&requests, 1));
    assert_eq!(queries_asked(&heard), [("pa", None), ("pars", None)], "not the one typed over");
}

#[test]
fn a_hub_too_old_to_find_isn_t_asked_again_until_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let requests = start_hub_answering(&socket, |request| {
        if matches!(request, ClientRequest::SessionFind(_)) {
            Some(refused(ErrorCode::Unsupported, "unknown request"))
        } else {
            answered_simply(request)
        }
    });
    let client = connect(&socket);
    let open = own(&client);
    let finished = |search: &Search| search.searched() != Searched::Underway;

    let first = open.search(&client, needle("parser"), 64);
    until("the first is answered", || finished(&first));
    let second = open.search(&client, needle("parsers"), 64);
    until("the second is answered", || finished(&second));
    // Kept, so the first connection is still open for the follower to let go of its subscription on.
    let again = connect(&socket);
    open.reconnect(&again);
    let third = open.search(&again, needle("parser"), 64);
    until("the third is answered", || finished(&third));

    for search in [&first, &second, &third] {
        assert_eq!(search.searched(), Searched::CantFind);
        assert!(search.take_error().is_none(), "it's no failure");
    }
    drop(open);
    let requests = requests_until_unsubscribed(&requests, 2);
    assert_eq!(queries_asked(&requests), [("parser", None); 2], "once on each connection: {requests:?}");
}

#[test]
fn a_query_too_short_isn_t_asked_and_a_failure_says_why_once() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let requests = start_hub_answering(&socket, |request| {
        if matches!(request, ClientRequest::SessionFind(_)) {
            Some(refused(ErrorCode::Internal, "the disk is full"))
        } else {
            answered_simply(request)
        }
    });
    let client = connect(&socket);
    let open = own(&client);

    let short = open.search(&client, needle(" a "), 64);
    let failing = open.search(&client, needle("parser"), 64);
    until("the failure is heard", || failing.searched() == Searched::Failed);

    assert_eq!(short.searched(), Searched::TooShort);
    assert!(matches!(failing.take_error(), Some(ClientError::Api(e)) if e.message == "the disk is full"));
    assert!(failing.take_error().is_none());
    drop(open);
    let requests = requests_until_unsubscribed(&requests, 1);
    assert_eq!(queries_asked(&requests), [("parser", None)]);
}

/// Such as one followed by `seq` because its hub is too old to read by place, and so to find.
#[test]
fn a_log_by_seq_isn_t_looked_through_at_the_hub() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let requests = start_hub_answering(&socket, answered_simply);
    let client = connect(&socket);
    let open = OpenSession::start(&client, &session());

    assert_eq!(open.read(SessionLog::by), By::Seq);
    assert_eq!(open.search(&client, needle("parser"), 64).searched(), Searched::CantFind);
    drop(open);
    assert!(queries_asked(&requests_until_unsubscribed(&requests, 1)).is_empty());
}

#[test]
fn a_log_that_holds_every_event_needs_no_hub_once_it_knows_how_far_they_go() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (answer, answered) = flume::unbounded::<()>();
    let requests = start_hub_answering(&socket, move |request| {
        if matches!(request, ClientRequest::SessionReadPlaces(_)) {
            answered.recv().unwrap();
            let placed =
                ClientResponse::PlacedEvents(log::PlacedEvents { events: Vec::new(), last_place: Place::new(1) });
            Some(ReplyOutcome::Response(placed))
        } else if matches!(request, ClientRequest::SessionFind(_)) {
            Some(found(&[]))
        } else {
            answered_simply(request)
        }
    });
    let client = connect(&socket);
    let open = own(&client);
    let parser = EventBody::Notice { level: NoticeLevel::Info, message: "parser".into(), parent_tool_use_id: None };
    let only = SessionEvent { place: Place::new(1), ..protocol::fixtures::event(Seq::new(1), parser) };

    let before = open.search(&client, needle("parser"), 64);
    answer.send(()).unwrap();
    until("it knows how far they go", || open.read(SessionLog::last) == at(1));
    open.follow.update(|log| log.apply_event(only));

    assert_eq!(open.search(&client, needle("parser"), 64).searched(), Searched::Done);
    until("the first is answered", || before.searched() == Searched::Done);
    drop(open);
    let requests = requests_until_unsubscribed(&requests, 1);
    assert_eq!(queries_asked(&requests), [("parser", None)], "only before it knew: {requests:?}");
}

#[test]
fn events_near_each_other_are_read_together_and_far_ones_apart() {
    let near = [(at(36)..at(215), at_each(&[100, 150])), (at(836)..at(965), at_each(&[900]))];

    assert_eq!(stretches(&at_each(&[100, 150, 900]), 64), near);
    assert_eq!(stretches(&at_each(&[10]), 64), [(at(1)..at(75), at_each(&[10]))], "from the first place");
    assert_eq!(stretches(&at_each(&[0]), 64), [], "and none for 0, where no event is");
}

#[test]
fn a_long_run_of_events_is_read_a_stretch_at_a_time_each_with_what_s_around_it() {
    let places: Vec<u64> = (100..5000).step_by(30).collect();

    let read = stretches(&at_each(&places), 64);

    assert!(read.len() > 1);
    let places_in = |range: &Range<Position>| range.start.place().get()..range.end.place().get();
    assert!(read.iter().all(|(range, _)| places_in(range).count() <= LONGEST), "{read:?}");
    let holding: Vec<Position> = read.iter().flat_map(|(_, holding)| holding.iter().copied()).collect();
    assert_eq!(holding, at_each(&places), "each once, in order");
    for (range, holding) in &read {
        let (range, held) = (places_in(range), holding.iter().map(|at| at.place().get()));
        assert!(held.clone().all(|at| range.start <= at - 64 && at + 64 < range.end), "{range:?} {holding:?}");
    }
}
