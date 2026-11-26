//! Replacing a worker of another build, against a hub that never starts a worker, with a stand-in for the one it
//! replaces, which `s` carries on with an agent on.

use std::path::Path;
use std::time::Instant;

use tokio::time::sleep_until;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{ObservationKey, RunId, SessionStatus};
use protocol::prompt::Prompt;
use protocol::worker::{GitStatus, WorkerRequest, WorkerResponse, previous};
use tokio::sync::broadcast::error::TryRecvError;

use super::*;
use crate::hub::observations::{Keeper, SessionToolServers, WorkspaceWorkerBuild};
use crate::hub::setup::SetupGate;
use crate::hub::tests::{carried_on, flushed, hub_in, hub_with_worktree, serving, stand_in_worker, terminal_in_w};
use crate::session::{Item, LiveSession, Origin};
use crate::versions;
use protocol::prompt::PromptFrom;

const RUN: RunId = RunId::new(7);
const TIMEOUT: Duration = Duration::from_secs(10);

fn w() -> WorkspaceId {
    "w".into()
}

fn prompt() -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("next"), from: PromptFrom::Person }
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
/// first goes on to the next worker, rather than the chat being interrupted and its queue held. What only its run kept,
/// its tool servers, goes as the session lets go of it, before any exit, which may never reach the hub.
#[tokio::test]
async fn a_prompt_recorded_as_the_worker_is_replaced_isnt_interrupted_by_its_agent_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, live) = carrying_on(dir.path());
    let servers = live.observed_as();
    let said =
        hub.observation_said_by_while::<SessionToolServers>(servers.clone(), Keeper::Run(RUN), Vec::new(), 1, || true);
    said.await.result().unwrap();

    assert!(hub.replace_if_idle(&w(), &link).await, "nothing was at work");
    assert!(hub.existing_worker(&w()).is_none(), "the worker is still the workspace's");
    assert!(!hub.observations.keeps::<SessionToolServers>(&servers), "its run's tool servers outlived it");
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
    waiting::task::until(TIMEOUT, "a call to the stand-in never ended", || link.calls_under_way() == 0).await;

    assert!(!hub.replace_if_idle(&w(), &link).await, "it was replaced");
    assert!(hub.existing_worker(&w()).is_some());
}

/// Its shell would end with it, which only closing the terminal or archiving the workspace does.
#[tokio::test]
async fn a_worker_with_a_terminal_open_isnt_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|_| Some(Ok(WorkerResponse::Ok)));
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();

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
    waiting::task::until(TIMEOUT, "the call never went", || link.calls_under_way() > 0).await;

    assert!(!hub.replace_if_idle(&w(), &link).await, "it was replaced");
    assert!(hub.existing_worker(&w()).is_some());
}

/// Each look comes [`crate::Timers::look_every`] after the last, and this checks halfway between them: idle at the
/// first, at work at the second, idle at the third, which follows one that wasn't, and replaced at the fourth. A setup
/// script is what's at work, since recording anything would let paused time run on while the record is written, and
/// there's no session, whose asking which account its agent runs under would be a call under way now and then.
#[tokio::test(start_paused = true)]
#[expect(
    clippy::disallowed_methods,
    reason = "tokio's clock is paused, so sleeping until halfway between looks moves it there"
)]
async fn a_worker_of_another_build_is_replaced_once_two_looks_in_a_row_find_nothing_at_work() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = with_worker(dir.path());
    let start = tokio::time::Instant::now();
    let every = hub.timers.look_every;
    let halfway_after = |look: u32| start + every * look + every / 2;
    let current = |hub: &Hub| hub.workers.lock().get(&w()).is_some_and(|current| Arc::ptr_eq(current, &link));

    hub.replace_once_idle(w(), link.clone()).await;
    assert!(hub.observations.keeps::<WorkspaceWorkerBuild>(&w()), "which build it is, kept before the looks start");
    let mut feed = hub.subscribe_feed();
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
    // The next worker is the hub's own build, so which build it was is forgotten as it goes, and every client hears it.
    let workspace = hub.workspace(&w()).await.unwrap();
    let provider = hub.provider(&workspace.provider).unwrap();
    hub.worker_gone(&w(), &link, provider.as_ref(), &workspace.instance_id).await;
    let gone = ObservationKey::WorkspaceWorkerBuild { workspace_id: w() };
    loop {
        match feed.try_recv().map(std::sync::Arc::unwrap_or_clone) {
            Ok(HubNotice::ObservationRemoved { observation, .. }) if observation == gone => break,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(e) => assert!(Instant::now() < deadline, "no client heard the workspace move to the hub's build: {e}"),
        }
        tokio::task::yield_now().await;
    }
    assert!(!hub.observations.keeps::<WorkspaceWorkerBuild>(&w()));
}

/// What the app needs to take an update that ends none of the workers: the oldest protocol among them, counting the
/// hub's own, since it goes on starting its own.
#[tokio::test]
async fn the_status_says_the_oldest_running_worker_protocol_counting_the_hub_s_own() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    assert_eq!(hub.status().oldest_running_worker_protocol, Some(versions::WORKERS.newest), "with no worker");

    stand_in_worker(&hub);
    assert_eq!(hub.status().oldest_running_worker_protocol, Some(versions::WORKERS.newest), "with one of its own");

    let refused = |_| Some(Err(ApiError::new(ErrorCode::Worker, "a stand-in does nothing")));
    let left = WorkerLink::answering_previous(refused);
    hub.workers.lock().insert("v".into(), left.clone());
    assert_eq!(hub.status().oldest_running_worker_protocol, Some(previous::VERSION), "with one an earlier hub left");

    // Still among them while whatever ends it gives back its instance, which a status read just then mustn't count.
    left.kill();
    assert_eq!(hub.status().oldest_running_worker_protocol, Some(versions::WORKERS.newest), "once it has closed");
}
