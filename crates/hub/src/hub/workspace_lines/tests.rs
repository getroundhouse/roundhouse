use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice, ObservationNotice};
use protocol::model::{
    Branches, ChangeStatus, ChangeSummary, ChangedFile, Head, LineCounts, Observation, ObservationKey, Observed,
    WorkspaceId,
};
use protocol::prompt::Prompt;
use protocol::worker::{SummarizeChanges, WorkerFrame, WorkerRequest, WorkerResponse};
use tokio::sync::{broadcast, oneshot};

use crate::hub::Hub;
use crate::hub::stand_in_workers::{TIMEOUT, a_chat, a_turn, answering_first, hub_with_stand_in_workers};
use crate::hub::tests::{archived, hub_with_worktree, serving};
use crate::store::now_ms;
use crate::worker_link::{Answer, WorkerLink};

/// What a worker answers of a worktree whose one changed file adds `added` lines and removes `removed`.
fn summary(added: u64, removed: u64) -> WorkerResponse {
    let lines = Some(LineCounts { added, removed });
    let file = ChangedFile { path: "a.rs".into(), from: None, status: ChangeStatus::Modified, lines, version: None };
    WorkerResponse::ChangeSummary(ChangeSummary::new("base".into(), vec![file]))
}

/// What a stand-in answers when asked to sum up its worktree's changes, as [`answering_first`] has it.
fn summing_up(
    answer: impl Fn() -> Result<WorkerResponse, ApiError> + Send + 'static,
) -> impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static {
    move |request| matches!(request, WorkerRequest::SummarizeChanges(_)).then(&answer)
}

/// The next notice the feed tells of an observation.
async fn next_observation(feed: &mut broadcast::Receiver<HubNotice>) -> ObservationNotice {
    let next = async {
        loop {
            if let Ok(notice) = ObservationNotice::try_from(feed.recv().await.unwrap()) {
                return notice;
            }
        }
    };
    tokio::time::timeout(TIMEOUT, next).await.expect("the feed told of an observation")
}

/// Each notice of an observation the feed has told since it was last read.
fn observations_told(feed: &mut broadcast::Receiver<HubNotice>) -> Vec<ObservationNotice> {
    let told = std::iter::from_fn(|| feed.try_recv().ok());
    told.filter_map(|notice| ObservationNotice::try_from(notice).ok()).collect()
}

/// Once the hub has finished reading the workspace's lines, for a test that counts what its worker is asked.
pub(in crate::hub) async fn read(hub: &Hub, id: &WorkspaceId) {
    waiting::task::until(TIMEOUT, "the hub to finish reading the lines", || !hub.line_reads.under_way(id)).await;
}

/// Each time a worker that kept what it was asked in `requests` was asked to sum up its changes.
fn summed_up(requests: &Mutex<Vec<WorkerRequest>>) -> Vec<SummarizeChanges> {
    requests.lock().iter().filter_map(summing_up_asked).collect()
}

/// What `request` asked, if it's to sum up a worktree's changes.
fn summing_up_asked(request: &WorkerRequest) -> Option<SummarizeChanges> {
    let WorkerRequest::SummarizeChanges(asked) = request else { return None };
    Some(asked.clone())
}

/// A turn that ends tells every client that follows the hub how many lines the workspace adds and removes, as of the
/// read, though no client has the workspace open, and has it listed for one that connects after.
#[tokio::test]
async fn a_turn_that_ends_has_every_client_sent_its_workspace_s_lines() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    answering_first(&hub, &workspace.id, &asked, summing_up(|| Ok(summary(3, 1))));
    let mut feed = hub.subscribe_feed();
    let before = now_ms();

    a_turn(&hub, &asked, &chat.id, Prompt::new("Change a.rs"), None).await;

    let ObservationNotice::Copy(told) = next_observation(&mut feed).await else { panic!("not a copy") };
    let Observation::WorkspaceLines { workspace_id, lines: Observed::AsOf { value, as_of_ms }, .. } = &told else {
        panic!("not the lines as of a read: {told:?}")
    };
    assert_eq!((workspace_id, *value), (&workspace.id, LineCounts { added: 3, removed: 1 }));
    assert!((before..=now_ms()).contains(as_of_ms), "read at {as_of_ms}, after {before}");
    assert_eq!(hub.records_list().await.unwrap().observations, [told]);
    let path = workspace.path.clone().unwrap();
    let wanted = SummarizeChanges { path, base_ref: workspace.base_ref.clone() };
    assert_eq!(asked.requests(summing_up_asked), [wanted]);
}

/// A worker from before the request refuses it, which leaves the lines not known and tells clients nothing.
#[tokio::test]
async fn a_worker_from_before_the_request_leaves_the_lines_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let unknown = || Err(ApiError::new(ErrorCode::Unsupported, "unknown request"));
    answering_first(&hub, &workspace.id, &asked, summing_up(unknown));
    let mut feed = hub.subscribe_feed();

    a_turn(&hub, &asked, &chat.id, Prompt::new("Change a.rs"), None).await;
    waiting::task::until(TIMEOUT, "the worker to be asked", || !asked.requests(summing_up_asked).is_empty()).await;
    read(&hub, &workspace.id).await;

    assert_eq!(observations_told(&mut feed), []);
    assert_eq!(hub.records_list().await.unwrap().observations, []);
}

/// Once a workspace is removed, as one that couldn't be made is, the feed says its lines are gone.
#[tokio::test]
async fn a_workspace_removed_has_its_lines_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let workspace = hub.workspace_create("r".into(), None, None, None).await.unwrap();
    answering_first(&hub, &workspace.id, &asked, summing_up(|| Ok(summary(3, 1))));
    let mut feed = hub.subscribe_feed();
    hub.read_lines(&workspace.id, &hub.existing_worker(&workspace.id).unwrap());
    let ObservationNotice::Copy(read) = next_observation(&mut feed).await else { panic!("not a copy") };

    let provider = hub.provider(&workspace.provider).unwrap();
    hub.discard_workspace(&workspace, provider.as_ref()).await;

    let ObservationNotice::Removed { observation, rev } = next_observation(&mut feed).await else {
        panic!("not a removal")
    };
    assert_eq!(observation, ObservationKey::WorkspaceLines { workspace_id: workspace.id });
    assert!(rev > read.rev());
    assert_eq!(hub.records_list().await.unwrap().observations, []);
}

/// What the worker says of its branches, which it says first as the hub takes it on, has the lines read, so a workspace
/// made before the hub read them has them without waiting for a turn. Not while it's archived, since its worktree is
/// gone, which leaves what was read as it was.
#[tokio::test]
async fn the_worker_s_word_of_its_branches_has_the_lines_read_while_the_workspace_is_ready() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let kept = requests.clone();
    let link = WorkerLink::answering(move |request| {
        kept.lock().push(request.clone());
        Some(summing_up(|| Ok(summary(2, 0)))(&request).unwrap_or(Ok(WorkerResponse::Ok)))
    });
    serving(&hub, &link);
    let id = WorkspaceId::from("w");
    let mut feed = hub.subscribe_feed();
    let frame = || WorkerFrame::Branches {
        branches: Branches { head: Head::Detached { commit: "c1".into() }, had: Vec::new() },
    };

    hub.branches_frame(&id, &link, frame()).await;

    let ObservationNotice::Copy(Observation::WorkspaceLines { lines: Observed::AsOf { value, .. }, .. }) =
        next_observation(&mut feed).await
    else {
        panic!("not a copy of the lines")
    };
    assert_eq!(value, LineCounts { added: 2, removed: 0 });

    archived(&hub);
    hub.branches_frame(&id, &link, frame()).await;
    read(&hub, &id).await;

    assert_eq!(summed_up(&requests).len(), 1, "an archived one's worktree is gone");
    assert_eq!(hub.records_list().await.unwrap().observations.len(), 1, "what was read stays");
}

/// A workspace is read once at a time, and something that says its lines may have changed while a read is under way
/// has them read once more after it, however often it says so, since that read may have begun before the change.
#[tokio::test]
async fn what_says_the_lines_changed_during_a_read_has_them_read_once_more_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (first, held) = oneshot::channel();
    let held = Mutex::new(Some(held));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let kept = requests.clone();
    let link = WorkerLink::answering_in_time(move |request| {
        kept.lock().push(request.clone());
        let WorkerRequest::SummarizeChanges(_) = request else { return Some(Answer::Now(Ok(WorkerResponse::Ok))) };
        Some(held.lock().take().map_or_else(|| Answer::Now(Ok(summary(5, 2))), Answer::Later))
    });
    serving(&hub, &link);
    let id = WorkspaceId::from("w");
    let mut feed = hub.subscribe_feed();
    hub.read_lines(&id, &link);
    waiting::task::until(TIMEOUT, "the first read to be asked", || summed_up(&requests).len() == 1).await;

    hub.read_lines(&id, &link);
    hub.read_lines(&id, &link);
    first.send(Ok(summary(1, 1))).unwrap();
    read(&hub, &id).await;

    assert_eq!(summed_up(&requests).len(), 2);
    let told = observations_told(&mut feed);
    let [.., ObservationNotice::Copy(Observation::WorkspaceLines { lines: Observed::AsOf { value, .. }, .. })] =
        told.as_slice()
    else {
        panic!("not the lines: {told:?}")
    };
    assert_eq!(*value, LineCounts { added: 5, removed: 2 });
}
