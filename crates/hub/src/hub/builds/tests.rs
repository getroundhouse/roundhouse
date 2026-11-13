//! Replacing a worker of another build, against a hub that never starts a worker, with a stand-in for the one it
//! replaces, which `s` carries on with an agent on.

use std::path::Path;
use std::time::Instant;

use tokio::time::sleep_until;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{RunId, SessionStatus};
use protocol::prompt::Prompt;
use protocol::worker::{GitStatus, WorkerRequest};
use tokio::sync::broadcast::error::TryRecvError;

use super::*;
use crate::hub::setup::SetupGate;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
use crate::session::{Item, LiveSession, Origin};

const RUN: RunId = RunId::new(7);
const TIMEOUT: Duration = Duration::from_secs(10);

fn w() -> WorkspaceId {
    "w".into()
}

fn prompt() -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("next") }
}

/// The hub, with a stand-in as `w`'s worker.
fn with_worker(dir: &Path) -> (Arc<Hub>, Arc<WorkerLink>) {
    let hub = hub_in(dir);
    let link = stand_in_worker(&hub);
    (hub, link)
}

/// The same, with `s` carrying on with an agent on it, which is idle. It asks which account the agent runs under,
/// which the stand-in doesn't say, so it asks again now and then.
fn carrying_on(dir: &Path) -> (Arc<Hub>, Arc<WorkerLink>, Arc<LiveSession>) {
    let (hub, link) = with_worker(dir);
    let live = carried_on(&hub, &link, RUN);
    (hub, link, live)
}

/// Stopping the worker brings its agent's exit, which is of an agent the session has let go of, so a prompt recorded
/// first goes on to the next worker, rather than the chat being interrupted and its queue held.
#[tokio::test]
async fn a_prompt_recorded_as_the_worker_is_replaced_isnt_interrupted_by_its_agent_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, live) = carrying_on(dir.path());

    assert!(hub.replace_if_idle(&w(), &link).await, "nothing was at work");
    assert!(hub.existing_worker(&w()).is_none(), "the worker is still the workspace's");
    live.record(Origin::Hub, prompt(), None).await.unwrap();
    let body = Box::new(EventBody::AgentExited {
        reason: ExitReason::Requested,
        code: None,
        signal: None,
        stderr_tail: vec![],
    });
    live.send_item(Item::Event { run: RUN, body, raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(&live).await;

    assert_eq!(live.status(), SessionStatus::Running);
}

#[tokio::test]
async fn a_worker_whose_workspace_has_a_session_at_work_isnt_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, live) = carrying_on(dir.path());

    live.record(Origin::Hub, prompt(), None).await.unwrap();
    // A prompt from the person has the hub read their account through the worker, which isn't what's at work here.
    let deadline = Instant::now() + TIMEOUT;
    while link.calls_under_way() > 0 {
        assert!(Instant::now() < deadline, "a call to the stand-in never ended");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(!hub.replace_if_idle(&w(), &link).await, "it was replaced");
    assert!(hub.existing_worker(&w()).is_some());
}

/// Such as a read of the worktree, which would fail if the worker stopped while it waits for the reply.
#[tokio::test]
async fn a_worker_with_a_call_under_way_isnt_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = WorkerLink::answering(|request| {
        let refused = Some(Err(ApiError::new(ErrorCode::Worker, "a stand-in does nothing")));
        if matches!(request, WorkerRequest::GitStatus(_)) { None } else { refused }
    });
    hub.workers.lock().insert(w(), link.clone());
    let calling = link.clone();
    tokio::spawn(async move { calling.call(GitStatus { path: "/w".into() }).await });
    let deadline = Instant::now() + TIMEOUT;
    while link.calls_under_way() == 0 {
        assert!(Instant::now() < deadline, "the call never went");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(!hub.replace_if_idle(&w(), &link).await, "it was replaced");
    assert!(hub.existing_worker(&w()).is_some());
}

/// Each look comes [`LOOK_EVERY`] after the last, and this checks halfway between them: idle at the first, at work at
/// the second, idle at the third, which follows one that wasn't, and replaced at the fourth. A setup script is what's
/// at work, since recording anything would let paused time run on while the record is written, and there's no session,
/// whose asking which account its agent runs under would be a call under way now and then.
#[tokio::test(start_paused = true)]
async fn a_worker_of_another_build_is_replaced_once_two_looks_in_a_row_find_nothing_at_work() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = with_worker(dir.path());
    let start = tokio::time::Instant::now();
    let halfway_after = |look: u32| start + LOOK_EVERY * look + LOOK_EVERY / 2;
    let current = |hub: &Hub| hub.workers.lock().get(&w()).is_some_and(|current| Arc::ptr_eq(current, &link));

    // As carrying on with it records, before the looks start.
    hub.store.set_workspace_worker_build(&w(), Some("0.9.0")).unwrap();
    let mut feed = hub.subscribe_feed();
    hub.replace_once_idle(w(), link.clone());
    sleep_until(halfway_after(1)).await;
    assert!(current(&hub), "replaced after one look");
    hub.setup_gates.lock().insert(w(), SetupGate::running());
    sleep_until(halfway_after(2)).await;
    hub.setup_gates.lock().remove(&w());
    sleep_until(halfway_after(3)).await;
    assert!(current(&hub), "replaced after a look that found a setup script running and one that didn't");
    sleep_until(halfway_after(4)).await;

    // Replacing it reads the workspace, which paused time could run on past. Yielding rather than sleeping holds it
    // still, so the look that replaces it is the fourth.
    let deadline = Instant::now() + TIMEOUT;
    while current(&hub) {
        assert!(Instant::now() < deadline, "not replaced after two looks in a row that found nothing at work");
        tokio::task::yield_now().await;
    }
    // The next worker is the hub's own build, so its workspace says so, and every client hears it.
    loop {
        match feed.try_recv() {
            Ok(HubNotice::Workspace(workspace)) if workspace.id == w() && workspace.worker_build.is_none() => break,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(e) => assert!(Instant::now() < deadline, "no client heard the workspace move to the hub's build: {e}"),
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(hub.store.get_workspace(&w()).unwrap().worker_build, None);
}
