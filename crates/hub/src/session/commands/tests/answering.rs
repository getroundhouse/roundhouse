//! When an answer to an approval is taken, as it's recorded.

use std::future::Future as _;
use std::task::{Context, Waker};

use protocol::capability::Capabilities;
use protocol::capability::approvals::Requests;
use protocol::event::EventBody;
use protocol::fixtures::{allow_once, approval_request, to_run};
use protocol::model::RunId;

use crate::hub::tests::{carried_on, flushed, hub_in, taking_worker};
use crate::session::progress::Step;
use crate::session::{Item, Origin};

/// The store has each event before the session takes it in, so a client can read a request in the log first, and an
/// answer it sends then is taken once the session has, rather than refused as answering nothing. It catches whether
/// it's waiting being asked as the answer arrives, before the answer waits its turn to be recorded.
#[tokio::test]
async fn an_answer_to_a_request_in_the_log_is_taken_however_soon_it_comes() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (link, asked) = taking_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let mut claims = Capabilities::default();
    claims.approvals.requests = Some(Requests { parts: Vec::new(), kept: Vec::new() });
    let init = EventBody::AgentInit {
        capabilities: Some(Box::new(claims)),
        agent_session_id: None,
        model: None,
        cwd: None,
        permission_mode: None,
        agent_version: None,
    };
    live.send_item(Item::Event { run, body: Box::new(init), raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(&live).await;
    let request = approval_request("a", to_run("ls")).into();

    // Recorded as the session records an event, and held between the store's commit and the session taking it in.
    let in_order = live.recording.lock().await;
    let origin = Origin::Run { run, current: true, numbered: None };
    let Step { body, change, next, .. } = live.state.lock().progress.step(origin, request, true);
    let id = live.id.clone();
    let appended = hub.blocking(move |store| store.append(&id, Some(run), &body, None, None, Some(change))).await;
    let appended = appended.unwrap();
    let mut answering = std::pin::pin!(live.handle_decide("a".into(), allow_once()));
    let early = answering.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    assert!(early.is_pending(), "it waits for what's recorded before it, rather than {early:?}");
    live.state.lock().progress.take(next, appended.event.seq, appended.event.ts_ms, appended.untaken.clone());
    drop(in_order);

    answering.await.unwrap();
    assert!(asked.lock().iter().any(|sent| sent.contains(r#""decide""#)), "and passes it on: {:?}", asked.lock());
}
