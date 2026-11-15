//! Telling a worker what the hub has recorded, as it runs and as it leaves the worker for the next hub.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::{RunId, Seq, SessionId};
use protocol::worker::{Received, WorkerRequest, WorkerResponse};
use tokio::time::Instant;

use super::acknowledge;
use crate::hub::Hub;
use crate::hub::tests::{carried_on, hub_in, serving, store_of};
use crate::session::Item;
use crate::worker_link::WorkerLink;

const RUN: RunId = RunId::new(1);

/// How long a hub that's leaving its worker gives it, which is far longer than anything these tests wait for, so
/// reaching it means the hub waited for an answer that never came.
const LEAVING: Duration = Duration::from_secs(15);

/// What a stand-in worker was told the hub had recorded through, each with the newest event the session `s` had
/// recorded then.
type Told = Arc<Mutex<Vec<(u64, Seq)>>>;

/// A stand-in for `w`'s worker, which answers every call, but what the hub says it has recorded only if `answers`, and
/// what it was told of that.
fn worker(hub: &Hub, answers: bool) -> (Arc<WorkerLink>, Told) {
    let told = Told::default();
    let (noted, store) = (told.clone(), store_of(hub).clone());
    let link = WorkerLink::answering(move |request| {
        if let WorkerRequest::Received(Received { through }) = request {
            noted.lock().push((through, store.get_session(&s()).unwrap().session.last_seq));
            return answers.then_some(Ok(WorkerResponse::Ok));
        }
        Some(Ok(WorkerResponse::Ok))
    });
    serving(hub, &link);
    (link, told)
}

fn s() -> SessionId {
    "s".into()
}

/// What the worker would send of what the agent of `s` said, numbered `seq`, as the hub's reader passes it on.
async fn said(hub: &Arc<Hub>, link: &WorkerLink, seq: u64) {
    let body = Box::new(EventBody::Notice { level: NoticeLevel::Info, message: "said".into(), subagent: None });
    hub.route(&s(), Item::Event { run: RUN, body, raw: None, attachments: Vec::new(), seq: Some(seq) }).await;
    link.acks.routed(&s(), seq);
}

async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

/// So a worker doesn't keep for good what its hub has recorded, and one that crashes leaves it only what came last,
/// which the next hub drops as recorded already. Only once it's recorded, or a hub that crashed first would lose it.
#[tokio::test]
async fn a_hub_tells_its_worker_what_it_has_recorded_once_it_has() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (link, told) = worker(&hub, true);
    let _live = carried_on(&hub, &link, RUN);
    tokio::spawn(acknowledge(hub.clone(), link.clone()));
    let before = store_of(&hub).get_session(&s()).unwrap().session.last_seq;

    said(&hub, &link, 1).await;

    wait_for("the worker to be told", || !told.lock().is_empty()).await;
    let (through, recorded) = told.lock()[0];
    assert_eq!(through, 1);
    assert!(recorded > before, "told before it was recorded");
}

/// A hub that leaves its worker while it's telling it what it recorded waits for that, rather than finding nothing
/// left to tell and closing first, which would leave the worker to send it all again to the next hub.
#[tokio::test]
#[expect(
    clippy::await_holding_lock,
    reason = "it holds the store's writer while the hub works, so the session's write waits, and nothing here writes"
)]
async fn a_hub_that_leaves_while_it_tells_its_worker_what_it_recorded_waits_for_that() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (link, told) = worker(&hub, true);
    let _live = carried_on(&hub, &link, RUN);
    tokio::spawn(acknowledge(hub.clone(), link.clone()));

    // So the session can't record it, and the hub can't tell the worker of it, until these go.
    let writes = store_of(&hub).holding_writes();
    said(&hub, &link, 1).await;
    wait_for("the hub to take it to tell of", || link.acks.routed.lock().through.is_none()).await;
    let leaving = tokio::spawn({
        let (hub, link) = (hub.clone(), link.clone());
        async move { link.leave(&hub, Instant::now() + LEAVING).await }
    });
    wait_for("the hub to start leaving", || link.left.load(Ordering::SeqCst)).await;
    drop(writes);
    leaving.await.unwrap();

    assert!(told.lock().iter().any(|(through, _)| *through == 1), "the worker was told nothing: {:?}", told.lock());
}

/// The worker lets go of what it's told of on a task of its own, so nothing but its answer says it has, before the
/// next hub's hello would have it send that again. A worker that never answers holds the hub up only so long.
#[tokio::test(start_paused = true)]
async fn a_hub_leaves_its_worker_once_it_has_said_it_let_go_of_what_was_recorded() {
    for answers in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_in(dir.path());
        let (link, told) = worker(&hub, answers);
        let _live = carried_on(&hub, &link, RUN);
        // As if something numbered 1 had gone to `s`, which records it before it answers a flush.
        link.acks.routed(&s(), 1);

        let deadline = Instant::now() + LEAVING;
        link.leave(&hub, deadline).await;

        let told: Vec<_> = told.lock().iter().map(|(through, _)| *through).collect();
        assert_eq!(told, [1], "told once, before it leaves");
        assert_eq!(Instant::now() < deadline, answers, "it waits for the answer, if one comes, and only until then");
        assert!(!link.calls.is_open(), "and then it leaves");
    }
}
