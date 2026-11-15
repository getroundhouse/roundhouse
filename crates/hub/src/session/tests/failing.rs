//! The agent's own failed requests in a row, which the hub says of each as it records it, even when the session is
//! started again between them, as when the hub starts again.

use super::super::{Item, LiveSession};
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, started_again};
use crate::session::progress::READ_AT_ONCE;
use protocol::capability::turns::OwnTurn;
use protocol::event::{Again, ContentBlock, EventBody, TurnOutcome};
use protocol::fixtures::request_failed;
use protocol::model::{RunId, Seq};

async fn send(live: &LiveSession, run: RunId, bodies: Vec<EventBody>) {
    for body in bodies {
        live.send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None }).await;
    }
    flushed(live).await;
}

/// Each of the agent's own failed requests, by its seq, with what the hub says it repeats.
async fn failures(hub: &crate::hub::Hub) -> Vec<(Seq, Option<Again>)> {
    let events = hub.blocking(|store| store.read_events(&"s".into(), Seq::FIRST, 1000, u64::MAX, false)).await.unwrap();
    events
        .iter()
        .filter_map(|event| {
            let EventBody::RequestFailed { subagent: None, again, .. } = &event.body else { return None };
            Some((event.seq, *again))
        })
        .collect()
}

/// More of the agent's own events that keep the row than a session started again reads at once.
fn more_than_is_read_at_once() -> impl Iterator<Item = EventBody> {
    let tasks = EventBody::AgentTasks { tasks: Vec::new(), stopped: Vec::new() };
    let (at_once, _) = READ_AT_ONCE;
    std::iter::repeat_n(tasks, usize::try_from(at_once).unwrap() + 6)
}

/// The third failure comes after the session was started again from what the store holds, with more of the agent's
/// own events between than are read at once, and a subagent's failure, which is no part of the row, after the second.
#[tokio::test]
async fn a_failure_after_the_session_starts_again_still_repeats_the_one_before() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let ended = || protocol::fixtures::turn_ended(TurnOutcome::Success);
    let mut before = vec![request_failed("limit", None), ended(), request_failed("limit", None), ended()];
    before.extend(more_than_is_read_at_once());
    before.push(request_failed("limit", Some("toolu_1")));
    send(&live, run, before).await;

    let started_again = started_again(&hub, &link, run);
    send(&started_again, run, vec![EventBody::OwnTurn(OwnTurn::TaskEnded { ended: Vec::new() })]).await;
    send(&started_again, run, vec![request_failed("limit", None)]).await;

    let [(first, None), (second, Some(again)), (_, third)] = failures(&hub).await[..] else { panic!() };
    assert_eq!(again, Again { previous: first, times: 2 });
    assert_eq!(third, Some(Again { previous: second, times: 3 }), "not the subagent's, nor counted from 1");
}

/// What ended the row ends it however far after the failure it comes, past what's read at once.
#[tokio::test]
async fn a_session_started_again_after_what_ended_the_row_starts_from_none() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let said = ContentBlock::Text { text: "Picking up.".into(), images: Vec::new() };
    let says =
        EventBody::Assistant { message_id: None, subagent: None, model: None, blocks: vec![said], first_index: None };
    let mut bodies = vec![request_failed("limit", None)];
    bodies.extend(more_than_is_read_at_once());
    bodies.push(says);
    send(&live, run, bodies).await;

    let started_again = started_again(&hub, &link, run);
    send(&started_again, run, vec![request_failed("limit", None)]).await;

    let [(_, None), (_, None)] = failures(&hub).await[..] else { panic!("the second repeats nothing") };
}
