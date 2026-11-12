//! Replacing a worker of another build, against a hub that never starts a worker, with a stand-in for the one it
//! replaces, which `s` carries on with an agent on.

use std::path::Path;
use std::time::Instant;

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{RunId, SessionId, SessionStatus};
use tokio::sync::oneshot;

use super::*;
use crate::hub::tests::hub_in;
use crate::session::{CarriedOver, Item, LiveSession, Origin};

const RUN: RunId = RunId::new(7);
const TIMEOUT: Duration = Duration::from_secs(10);

fn w() -> WorkspaceId {
    "w".into()
}

fn prompt() -> EventBody {
    EventBody::UserPrompt { text: "next".into(), client_msg_id: None, attachments: Vec::new() }
}

/// The hub, with a stand-in as `w`'s worker, and `s` carrying on with an agent on it, which is idle.
fn carrying_on(dir: &Path) -> (Arc<Hub>, Arc<WorkerLink>, Arc<LiveSession>) {
    let hub = hub_in(dir);
    let link = WorkerLink::answering(|_| Some(Err(ApiError::new(ErrorCode::Worker, "a stand-in does nothing"))));
    hub.workers.lock().insert(w(), link.clone());
    let id: SessionId = "s".into();
    let stored = hub.store.get_session(&id).unwrap();
    let carried = CarriedOver { link: link.id, run: RUN, recorded: None, untaken: Vec::new() };
    let live = hub.sessions.lock().keep(&id, || LiveSession::start(&hub, stored, Vec::new(), Some(carried))).unwrap();
    (hub, link, live)
}

/// Stopping the worker brings its agent's exit, which is of an agent the session has let go of, so a prompt recorded
/// first goes on to the next worker, rather than the chat being interrupted and its queue held.
#[tokio::test]
async fn a_prompt_recorded_as_the_worker_is_replaced_isnt_interrupted_by_its_agent_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, live) = carrying_on(dir.path());

    assert!(hub.replace_if_unused(&w(), &link, link.uses()).await, "nothing was at work");
    assert!(hub.existing_worker(&w()).is_none(), "the worker is still the workspace's");
    live.record(Origin::Hub, prompt(), None, None).await.unwrap();
    let body = Box::new(EventBody::AgentExited {
        reason: ExitReason::Requested,
        code: None,
        signal: None,
        stderr_tail: vec![],
    });
    live.send_item(Item::Event { run: RUN, body, raw: None, attachments: Vec::new(), seq: None }).await;
    let (flushed, taken) = oneshot::channel();
    live.send_item(Item::Flush(flushed)).await;
    taken.await.unwrap();

    assert_eq!(live.status(), SessionStatus::Running);
}

/// Whatever it was handed to may not have started its work yet.
#[tokio::test]
async fn a_worker_handed_out_since_the_hub_looked_isnt_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _live) = carrying_on(dir.path());
    let uses = link.uses();

    let handed = hub.existing_worker(&w()).unwrap();

    assert!(!hub.replace_if_unused(&w(), &link, uses).await, "it was replaced");
    assert!(hub.existing_worker(&w()).is_some_and(|current| Arc::ptr_eq(&current, &handed)));
}

#[tokio::test]
async fn a_worker_whose_workspace_has_a_session_at_work_isnt_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, live) = carrying_on(dir.path());

    live.record(Origin::Hub, prompt(), None, None).await.unwrap();
    // A prompt from the person has the hub read their account through the worker, which isn't what's at work here.
    let deadline = Instant::now() + TIMEOUT;
    while link.calls_under_way() > 0 {
        assert!(Instant::now() < deadline, "a call to the stand-in never ended");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(!hub.replace_if_unused(&w(), &link, link.uses()).await, "it was replaced");
    assert!(hub.existing_worker(&w()).is_some());
}
