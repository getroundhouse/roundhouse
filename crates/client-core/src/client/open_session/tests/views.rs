//! Following a view of a session on a hub from before places, and on one from now.

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::time::{Duration, Instant};

use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::event::{EventBody, NoticeLevel, SessionEvent};
use protocol::frame;
use protocol::model::{Place, Session, ToolUseId};
use serde_json::json;

use super::super::{OpenSession, View};
use crate::Client;
use crate::session_log::{By, Row, SessionLog};

/// When a scripted hub is from: before it could read the agent's own events apart, or now.
#[derive(Clone, Copy, PartialEq)]
enum Era {
    BeforePlaces,
    Now,
}

/// The session's events: the agent's own at the odd `seq`s, and a subagent's among them at the even ones, so no event
/// but the first has its `seq` as its place. A hub from before places recorded neither.
fn recorded(seq: u64, era: Era) -> SessionEvent {
    let (subagent, place) = match era {
        Era::BeforePlaces => (None, Place::default()),
        Era::Now if seq.is_multiple_of(2) => (Some("call".into()), Place::new(seq / 2)),
        Era::Now => (None, Place::new(seq.div_ceil(2))),
    };
    SessionEvent {
        session_id: "s".into(),
        seq,
        ts_ms: 0,
        run: None,
        subagent,
        place,
        body: EventBody::Notice { level: NoticeLevel::Info, message: format!("{seq}") },
        raw: None,
    }
}

const LAST_SEQ: u64 = 5;
/// The agent's own events, by `seq`.
const OWN: [u64; 3] = [1, 3, 5];
const EVERY: [u64; 5] = [1, 2, 3, 4, 5];

/// A hub that serves its session's events, by seq, and by place if it's from `era` [`Era::Now`]. It refuses whatever
/// else it's asked. It says each request it gets, as it gets it.
fn start_hub(socket: &Path, era: Era) -> flume::Receiver<ClientRequest> {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    let (asked, requests) = flume::unbounded();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
        while let Some(Ok(line)) = lines.next() {
            let ClientFrame { id, request } = frame::decode(line.as_bytes()).unwrap();
            // Nobody may be listening, for a hub whose requests a test doesn't look at.
            let _ = asked.send(request.clone());
            let mut send = |frame: &HubFrame| stream.write_all(&frame::encode(frame).unwrap()).unwrap();
            let outcome = |response| HubFrame::Reply { id, outcome: ReplyOutcome::Response(response) };
            let events_from = |from: u64| (from..=LAST_SEQ).map(|seq| recorded(seq, era));
            let placed_from = |subagent: &Option<ToolUseId>, from: Place| {
                let of = |event: &SessionEvent| event.subagent == *subagent;
                let events: Vec<SessionEvent> = events_from(1).filter(of).collect();
                let last = events.last().map_or(Place::default(), |event| event.place);
                (events.into_iter().filter(|event| event.place >= from).collect::<Vec<_>>(), last)
            };
            #[expect(clippy::wildcard_enum_match_arm, reason = "a hub refuses every request it doesn't know")]
            match request {
                ClientRequest::Hello { .. } => {
                    send(&outcome(ClientResponse::Hello {
                        protocol_version: CLIENT_PROTOCOL_VERSION,
                        hub_version: "o".into(),
                    }));
                }
                ClientRequest::SessionSubscribe { from_seq, subscription_id, .. } => {
                    send(&outcome(ClientResponse::Ok));
                    for event in events_from(from_seq) {
                        send(&HubFrame::Event { subscription_id, event });
                    }
                }
                ClientRequest::SessionSubscribePlaces { subagent, from_place, subscription_id, .. }
                    if era == Era::Now =>
                {
                    send(&outcome(ClientResponse::Ok));
                    for event in placed_from(&subagent, from_place).0 {
                        send(&HubFrame::Event { subscription_id, event });
                    }
                }
                ClientRequest::SessionRead { from_seq, .. } => {
                    let events = events_from(from_seq).collect();
                    send(&outcome(ClientResponse::Events { events, last_seq: LAST_SEQ }));
                }
                ClientRequest::SessionReadPlaces { subagent, from_place, limit, .. } if era == Era::Now => {
                    let (events, last_place) = placed_from(&subagent, from_place);
                    let events = events.into_iter().take(limit as usize).collect();
                    send(&outcome(ClientResponse::PlacedEvents { events, last_place }));
                }
                _ => {
                    let refused = ApiError::new(ErrorCode::Unsupported, "this hub doesn't know that request");
                    send(&HubFrame::Reply { id, outcome: ReplyOutcome::Error(refused) });
                }
            }
        }
    });
    requests
}

/// The `seq` of each event `open` holds, row by row, or `None` for a row it doesn't hold.
fn rows(open: &OpenSession) -> Vec<Option<u64>> {
    open.read(|log| {
        let row = |ix| match log.row(ix) {
            Row::Event(event) => Some(event.seq),
            Row::Unloaded => None,
        };
        (0..log.row_count()).map(row).collect()
    })
}

/// Waits until `open` holds exactly the events at `seqs`, one to a row, in that order.
fn wait_for(open: &OpenSession, seqs: &[u64]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let wanted: Vec<Option<u64>> = seqs.iter().copied().map(Some).collect();
    while rows(open) != wanted {
        assert!(Instant::now() < deadline, "never held {seqs:?}, only {:?}", rows(open));
        std::thread::sleep(Duration::from_millis(10));
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
    serde_json::from_value(json!({
        "id": "s", "workspace_id": "w", "agent": "claude", "model": "opus", "permission_mode": "bypassPermissions", "effort": "xhigh",
        "status": "idle", "last_seq": LAST_SEQ, "created_at_ms": 0, "updated_at_ms": 0,
    }))
    .unwrap()
}

#[test]
fn the_agents_own_events_from_a_hub_that_cant_read_them_apart_are_every_event_by_seq() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = start_hub(&socket, Era::BeforePlaces);
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenSession::start_view(&client, &session(), View::Own);

    wait_for(&open, &EVERY);
    assert_eq!(open.read(SessionLog::by), By::Seq);
    let requests: Vec<ClientRequest> = asked.drain().collect();
    let by_place = requests.iter().position(|r| matches!(r, ClientRequest::SessionReadPlaces { .. }));
    let by_seq = requests.iter().position(|r| matches!(r, ClientRequest::SessionSubscribe { from_seq: 1, .. }));
    assert!(by_place.zip(by_seq).is_some_and(|(place, seq)| place < seq), "by place first, then by seq: {requests:?}");
}

#[test]
fn an_open_view_reads_by_place_again_once_it_reaches_a_hub_that_can() {
    let dir = tempfile::tempdir().unwrap();
    let (older, newer) = (dir.path().join("older.sock"), dir.path().join("newer.sock"));
    start_hub(&older, Era::BeforePlaces);
    let asked = start_hub(&newer, Era::Now);
    let open = OpenSession::start_view(&Client::connect(&older, "test").unwrap(), &session(), View::Own);
    wait_for(&open, &EVERY);

    open.reconnect(&Client::connect(&newer, "test").unwrap());

    wait_for(&open, &OWN);
    assert_eq!(open.read(SessionLog::by), By::Place);
    let requests: Vec<ClientRequest> = asked.drain().collect();
    assert!(requests.iter().any(|r| matches!(r, ClientRequest::SessionSubscribePlaces { .. })), "{requests:?}");
}

#[test]
fn an_open_view_carries_on_by_seq_from_one_hub_that_cant_read_by_place_to_another() {
    let dir = tempfile::tempdir().unwrap();
    let (first_hub, second_hub) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    start_hub(&first_hub, Era::BeforePlaces);
    let asked = start_hub(&second_hub, Era::BeforePlaces);
    let open = OpenSession::start_view(&Client::connect(&first_hub, "test").unwrap(), &session(), View::Own);
    wait_for(&open, &EVERY);
    let added = open.read(SessionLog::events_added);

    open.reconnect(&Client::connect(&second_hub, "test").unwrap());

    let from =
        first(
            &asked,
            |r| {
                if let ClientRequest::SessionSubscribe { from_seq, .. } = r { Some(*from_seq) } else { None }
            },
        );
    assert_eq!(from, LAST_SEQ + 1, "it carries on from the last event it has");
    assert_eq!(open.read(SessionLog::events_added), added, "and starts nothing again");
    assert_eq!(rows(&open), EVERY.map(Some));
}
