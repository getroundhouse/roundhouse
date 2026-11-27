//! Following a view of a session: the agent's own events, or a subagent's, read again from a hub of another build.

use crate::Hub;
use std::path::Path;
use std::time::Duration;

use protocol::client::subscriptions::{Follow, LogFrom};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome, log};
use protocol::event::{EventBody, NoticeLevel, SessionEvent};
use protocol::model::{Place, Seq, Session, SubagentId};

use super::super::{OpenSession, View};
use super::follows_session;
use crate::Connection;
use crate::session_log::Row;
use crate::stand_in_hub::{Does, StandIn, subscribed};

/// The session's events: the agent's own at the odd `seq`s, and a subagent's among them at the even ones, so no event
/// but the first has its `seq` as its place. Each says its `seq` and the build of the hub that served it, as a
/// migration can rewrite an event.
fn recorded(seq: u64, build: &str) -> SessionEvent {
    let (subagent, place) = if seq.is_multiple_of(2) {
        (Some("call".into()), Place::new(seq / 2))
    } else {
        (None, Place::new(seq.div_ceil(2)))
    };
    let message = format!("{seq} from {build}");
    let body = EventBody::Notice { level: NoticeLevel::Info, message, subagent: None };
    SessionEvent { subagent, place, ..protocol::fixtures::event(Seq::new(seq), body) }
}

const LAST_SEQ: u64 = 5;
/// The agent's own events, by `seq`.
const OWN: [u64; 3] = [1, 3, 5];
const THE_SUBAGENTS: [u64; 2] = [2, 4];

/// A hub of `build` that serves its session's events, up to `last`, by place. It refuses whatever else it's asked. It
/// says each request it gets, as it gets it.
fn start_hub(socket: &Path, build: &'static str, last: u64) -> flume::Receiver<ClientRequest> {
    let placed_from = move |subagent: &Option<SubagentId>, from: Place| {
        let of = |event: &SessionEvent| event.subagent == *subagent;
        let events: Vec<SessionEvent> = (1..=last).map(|seq| recorded(seq, build)).filter(of).collect();
        let last = events.last().map_or(Place::default(), |event| event.place);
        (events.into_iter().filter(|event| event.place >= from).collect::<Vec<_>>(), last)
    };
    let answers = |response| Does::Answers(Box::new(ReplyOutcome::Response(response)));
    let follows = move |subscription_id, from: Option<&LogFrom>| {
        let Some(LogFrom::Place { subagent, place }) = from else { return Vec::new() };
        let events = placed_from(subagent, *place).0;
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
            ClientRequest::SessionReadPlaces(log::SessionReadPlaces { subagent, from_place, limit, .. }) => {
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

/// The place a request to follow events starts from, or `None` for any other request.
fn subscribed_from(request: &ClientRequest) -> Option<u64> {
    let LogFrom::Place { place, .. } = follows_session(request)? else { return None };
    Some(place.get())
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

/// As after an update whose migration rewrote the events the view holds, which catching up from one past the last
/// would leave as they were. Another hub of that build is caught up with, from one past the last.
#[test]
fn an_open_view_reads_its_events_again_from_a_hub_of_another_build_once() {
    let cases = [(View::Own, OWN.as_slice()), (View::Subagent("call".into()), THE_SUBAGENTS.as_slice())];
    for (view, held) in cases {
        let dir = tempfile::tempdir().unwrap();
        let socket = |name: &str| dir.path().join(name);
        start_hub(&socket("before.sock"), "1", LAST_SEQ);
        start_hub(&socket("after.sock"), "2", LAST_SEQ);
        let asked = start_hub(&socket("again.sock"), "2", LAST_SEQ);
        let client = Connection::connect(&socket("before.sock"), "test").unwrap();
        let hub = Hub::caught_up_on(&client);
        let open = hub.open_session(&session(), view.clone());
        wait_for(&open, held);

        hub.carry_on_to(&Connection::connect(&socket("after.sock"), "test").unwrap());

        let again: Vec<Option<String>> = held.iter().map(|seq| Some(format!("{seq} from 2"))).collect();
        wait_until(&open, said, &again);
        hub.carry_on_to(&Connection::connect(&socket("again.sock"), "test").unwrap());
        let next = u64::try_from(held.len()).unwrap() + 1;
        assert_eq!(first(&asked, subscribed_from), next, "{view:?} carries on from the last it holds");
    }
}

/// As when the agent went on while the hub restarted to update, so the new hub's events go more than a page further
/// than those the view held. The page above the newest is read again too, as the view looks at it.
#[test]
fn an_open_view_reads_its_events_again_as_far_as_the_new_hub_has_them() {
    let dir = tempfile::tempdir().unwrap();
    let (before, after) = (dir.path().join("before.sock"), dir.path().join("after.sock"));
    // The agent's own events are every other one, so 740 of them, then 755, and the one in row 300 is at 601.
    start_hub(&before, "1", 1480);
    start_hub(&after, "2", 1510);
    let client = Connection::connect(&before, "test").unwrap();
    let hub = Hub::caught_up_on(&client);
    let open = hub.open_session(&session(), View::Own);
    let row_300 = |open: &OpenSession| said(open).get(300).cloned().flatten();
    open.want(300..301, Some(300));
    wait_until(&open, row_300, &Some("601 from 1".into()));

    hub.carry_on_to(&Connection::connect(&after, "test").unwrap());
    open.want(300..301, Some(300));

    wait_until(&open, row_300, &Some("601 from 2".into()));
}
