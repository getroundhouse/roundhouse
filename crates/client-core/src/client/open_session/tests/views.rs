//! Following a view of a session on a hub from before places, and on one from now.

use std::path::Path;
use std::time::Duration;

use protocol::client::subscriptions::{Follow, LogFrom};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome, log};
use protocol::event::{EventBody, NoticeLevel, SessionEvent};
use protocol::model::{Place, Seq, Session, SubagentId};

use super::super::{OpenSession, View};
use super::follows_session;
use crate::session_log::{By, Row, SessionLog};
use crate::stand_in_hub::{Does, StandIn, subscribed};
use crate::{Client, RecordsFeed};

/// When a scripted hub is from: before it could read the agent's own events apart, or now.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Era {
    BeforePlaces,
    Now,
}

/// The session's events: the agent's own at the odd `seq`s, and a subagent's among them at the even ones, so no event
/// but the first has its `seq` as its place. A hub from before places recorded neither. Each says its `seq` and the
/// build of the hub that served it, as a migration can rewrite an event.
fn recorded(seq: u64, era: Era, build: &str) -> SessionEvent {
    let (subagent, place) = match era {
        Era::BeforePlaces => (None, Place::default()),
        Era::Now if seq.is_multiple_of(2) => (Some("call".into()), Place::new(seq / 2)),
        Era::Now => (None, Place::new(seq.div_ceil(2))),
    };
    let message = format!("{seq} from {build}");
    let body = EventBody::Notice { level: NoticeLevel::Info, message, subagent: None };
    SessionEvent { subagent, place, ..protocol::fixtures::event(Seq::new(seq), body) }
}

const LAST_SEQ: u64 = 5;
/// The agent's own events, by `seq`.
const OWN: [u64; 3] = [1, 3, 5];
const THE_SUBAGENTS: [u64; 2] = [2, 4];
const EVERY: [u64; 5] = [1, 2, 3, 4, 5];

/// A hub of `build` that serves its session's events, up to `last`, by seq, and by place if it's from `era`
/// [`Era::Now`]. It refuses whatever else it's asked. It says each request it gets, as it gets it.
fn start_hub(socket: &Path, era: Era, build: &'static str, last: u64) -> flume::Receiver<ClientRequest> {
    let events_from = move |from: Seq| (from.get()..=last).map(move |seq| recorded(seq, era, build));
    let placed_from = move |subagent: &Option<SubagentId>, from: Place| {
        let of = |event: &SessionEvent| event.subagent == *subagent;
        let events: Vec<SessionEvent> = events_from(Seq::FIRST).filter(of).collect();
        let last = events.last().map_or(Place::default(), |event| event.place);
        (events.into_iter().filter(|event| event.place >= from).collect::<Vec<_>>(), last)
    };
    let answers = |response| Does::Answers(Box::new(ReplyOutcome::Response(response)));
    let follows = move |subscription_id, from: Option<&LogFrom>| {
        let events = match from {
            None => events_from(Seq::FIRST).collect(),
            Some(LogFrom::Seq { seq }) => events_from(*seq).collect(),
            Some(LogFrom::Place { subagent, place }) => placed_from(subagent, *place).0,
            Some(LogFrom::Unknown) => Vec::new(),
        };
        events.into_iter().map(|event| HubFrame::Event { subscription_id, event }).collect()
    };
    StandIn::of_build(build).listen(socket, move |request| {
        #[expect(clippy::wildcard_enum_match_arm, reason = "a hub refuses every request it doesn't know")]
        match request {
            ClientRequest::Subscribe(subscribe) if subscribe.start.is_empty() => Does::Nothing,
            ClientRequest::Subscribe(subscribe) => subscribed(&subscribe, |subscription_id, follow| match follow {
                Follow::Session { from, .. } => follows(subscription_id, from.as_ref()),
                _ => Vec::new(),
            }),
            ClientRequest::SessionRead(log::SessionRead { from_seq, .. }) => {
                let events = events_from(from_seq).collect();
                answers(ClientResponse::Events(log::Events { events, last_seq: Seq::new(last) }))
            }
            ClientRequest::SessionReadPlaces(log::SessionReadPlaces { subagent, from_place, limit, .. })
                if era == Era::Now =>
            {
                let (events, last_place) = placed_from(&subagent, from_place);
                let events = events.into_iter().take(limit as usize).collect();
                answers(ClientResponse::PlacedEvents(log::PlacedEvents { events, last_place }))
            }
            _ => {
                let refused = ApiError::new(ErrorCode::Unsupported, "this hub doesn't know that request");
                Does::Answers(Box::new(ReplyOutcome::Error(refused)))
            }
        }
    })
}

/// What `open` holds, row by row, as `of` reads each event, or `None` for a row it doesn't hold.
fn held<T>(open: &OpenSession, of: impl Fn(&SessionEvent) -> T) -> Vec<Option<T>> {
    open.read(|log| {
        let row = |ix| match log.row(ix) {
            Row::Event(event) => Some(of(event)),
            Row::Unloaded => None,
        };
        Vec::from_fn(log.row_count(), row)
    })
}

/// The `seq` of each event `open` holds, row by row, or `None` for a row it doesn't hold.
fn rows(open: &OpenSession) -> Vec<Option<u64>> {
    held(open, |event| event.seq.get())
}

/// What each event `open` holds says, row by row, or `None` for a row it doesn't hold.
fn said(open: &OpenSession) -> Vec<Option<String>> {
    held(open, |event| {
        let EventBody::Notice { message, .. } = &event.body else { panic!("the hub records only notices") };
        message.clone()
    })
}

/// Waits until `read` says `wanted` of `open`.
fn wait_until<T: PartialEq + std::fmt::Debug>(open: &OpenSession, read: impl Fn(&OpenSession) -> T, wanted: &T) {
    waiting::thread::until_ok(Duration::from_secs(10), || {
        let now = read(open);
        if now == *wanted { Ok(()) } else { Err(format!("never {wanted:?}, only {now:?}")) }
    });
}

/// Waits until `open` holds exactly the events at `seqs`, one to a row, in that order.
fn wait_for(open: &OpenSession, seqs: &[u64]) {
    wait_until(open, rows, &seqs.iter().copied().map(Some).collect());
}

/// Where a request to follow events starts, by `seq` or by place, or `None` for any other request.
fn subscribed_from(request: &ClientRequest) -> Option<u64> {
    match follows_session(request)? {
        LogFrom::Seq { seq } => Some(seq.get()),
        LogFrom::Place { place, .. } => Some(place.get()),
        LogFrom::Unknown => None,
    }
}

/// The first request among `requests` that `pick` takes.
fn first<T>(requests: &flume::Receiver<ClientRequest>, pick: impl Fn(&ClientRequest) -> Option<T>) -> T {
    loop {
        let request = requests.recv_timeout(Duration::from_secs(10)).expect("the request never came");
        if let Some(found) = pick(&request) {
            return found;
        }
    }
}

fn session() -> Session {
    Session { last_seq: Seq::new(LAST_SEQ), ..protocol::fixtures::session("s") }
}

#[test]
fn the_agents_own_events_from_a_hub_that_cant_read_them_apart_are_every_event_by_seq() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = start_hub(&socket, Era::BeforePlaces, "1", LAST_SEQ);
    let client = Client::connect(&socket, "test").unwrap();

    let feed = RecordsFeed::start(&client);
    let open = OpenSession::start_view(&client, &feed, &session(), View::Own);

    wait_for(&open, &EVERY);
    assert_eq!(open.read(SessionLog::by), By::Seq);
    let requests: Vec<ClientRequest> = asked.drain().collect();
    let by_place = requests.iter().position(|r| matches!(r, ClientRequest::SessionReadPlaces(_)));
    let by_seq = requests.iter().position(|r| follows_session(r) == Some(LogFrom::Seq { seq: Seq::FIRST }));
    assert!(by_place.zip(by_seq).is_some_and(|(place, seq)| place < seq), "by place first, then by seq: {requests:?}");
}

#[test]
fn an_open_view_reads_by_place_again_once_it_reaches_a_hub_that_can() {
    let dir = tempfile::tempdir().unwrap();
    let (older, newer) = (dir.path().join("older.sock"), dir.path().join("newer.sock"));
    start_hub(&older, Era::BeforePlaces, "1", LAST_SEQ);
    let asked = start_hub(&newer, Era::Now, "1", LAST_SEQ);
    let client = Client::connect(&older, "test").unwrap();
    let feed = RecordsFeed::start(&client);
    let open = OpenSession::start_view(&client, &feed, &session(), View::Own);
    wait_for(&open, &EVERY);

    open.reconnect(&Client::connect(&newer, "test").unwrap());

    wait_for(&open, &OWN);
    assert_eq!(open.read(SessionLog::by), By::Place);
    let requests: Vec<ClientRequest> = asked.drain().collect();
    let by_place = |r: &ClientRequest| matches!(follows_session(r), Some(LogFrom::Place { .. }));
    assert!(requests.iter().any(by_place), "{requests:?}");
}

#[test]
fn an_open_view_carries_on_by_seq_from_one_hub_that_cant_read_by_place_to_another() {
    let dir = tempfile::tempdir().unwrap();
    let (first_hub, second_hub) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    start_hub(&first_hub, Era::BeforePlaces, "1", LAST_SEQ);
    let asked = start_hub(&second_hub, Era::BeforePlaces, "1", LAST_SEQ);
    let client = Client::connect(&first_hub, "test").unwrap();
    let feed = RecordsFeed::start(&client);
    let open = OpenSession::start_view(&client, &feed, &session(), View::Own);
    wait_for(&open, &EVERY);
    let added = open.read(SessionLog::events_added);

    open.reconnect(&Client::connect(&second_hub, "test").unwrap());

    let from = first(&asked, |r| {
        let LogFrom::Seq { seq } = follows_session(r)? else { return None };
        Some(seq)
    });
    assert_eq!(from, Seq::new(LAST_SEQ).next(), "it carries on from the last event it has");
    assert_eq!(open.read(SessionLog::events_added), added, "and starts nothing again");
    assert_eq!(rows(&open), EVERY.map(Some));
}

/// As after an update whose migration rewrote the events the view holds, which catching up from one past the last
/// would leave as they were. Another hub of that build is caught up with, from one past the last.
#[test]
fn an_open_view_reads_its_events_again_from_a_hub_of_another_build_once() {
    let cases = [
        (Era::Now, View::Own, OWN.as_slice()),
        (Era::Now, View::Subagent("call".into()), THE_SUBAGENTS.as_slice()),
        (Era::Now, View::Whole, EVERY.as_slice()),
        (Era::BeforePlaces, View::Own, EVERY.as_slice()),
        (Era::BeforePlaces, View::Whole, EVERY.as_slice()),
    ];
    for (era, view, held) in cases {
        let dir = tempfile::tempdir().unwrap();
        let socket = |name: &str| dir.path().join(name);
        start_hub(&socket("before.sock"), era, "1", LAST_SEQ);
        start_hub(&socket("after.sock"), era, "2", LAST_SEQ);
        let asked = start_hub(&socket("again.sock"), era, "2", LAST_SEQ);
        let client = Client::connect(&socket("before.sock"), "test").unwrap();
        let feed = RecordsFeed::start(&client);
        let open = OpenSession::start_view(&client, &feed, &session(), view.clone());
        wait_for(&open, held);

        open.reconnect(&Client::connect(&socket("after.sock"), "test").unwrap());

        let again: Vec<Option<String>> = held.iter().map(|seq| Some(format!("{seq} from 2"))).collect();
        wait_until(&open, said, &again);
        open.reconnect(&Client::connect(&socket("again.sock"), "test").unwrap());
        let next = u64::try_from(held.len()).unwrap() + 1;
        assert_eq!(first(&asked, subscribed_from), next, "{era:?} {view:?} carries on from the last it holds");
    }
}

/// As when the agent went on while the hub restarted to update, so the new hub's events go more than a page further
/// than those the view held. The page above the newest is read again too, as the view looks at it.
#[test]
fn an_open_view_reads_its_events_again_as_far_as_the_new_hub_has_them() {
    let dir = tempfile::tempdir().unwrap();
    let (before, after) = (dir.path().join("before.sock"), dir.path().join("after.sock"));
    // The agent's own events are every other one, so 740 of them, then 755, and the one in row 300 is at 601.
    start_hub(&before, Era::Now, "1", 1480);
    start_hub(&after, Era::Now, "2", 1510);
    let client = Client::connect(&before, "test").unwrap();
    let feed = RecordsFeed::start(&client);
    let open = OpenSession::start_view(&client, &feed, &session(), View::Own);
    let row_300 = |open: &OpenSession| said(open).get(300).cloned().flatten();
    open.want(300..301, Some(300));
    wait_until(&open, row_300, &Some("601 from 1".into()));

    open.reconnect(&Client::connect(&after, "test").unwrap());
    open.want(300..301, Some(300));

    wait_until(&open, row_300, &Some("601 from 2".into()));
}
