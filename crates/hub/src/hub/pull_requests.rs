//! Each workspace's pull request, which the hub keeps on the workspace's record, so every client shows every
//! workspace's as soon as it connects, and as it last was after a restart. It changes on GitHub while nobody's looking,
//! so the hub reads it again when a client asks, when an agent's turn ends, and on its own, within what GitHub can
//! spare.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use budget::{Budget, calls_to_read};
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{PullRequest, PullRequestFound, Workspace, WorkspaceId, WorkspaceStatus};
use protocol::worker::{WorkerRequest, WorkerResponse};
use schedule::{Schedule, Went};
use tokio::sync::Mutex as AsyncMutex;
use tracing::debug;

use super::{Hub, unexpected};
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

mod budget;
mod schedule;

/// How often the hub looks for a pull request to read that nobody asked for, while the budget can spare the calls.
const PACE: Duration = Duration::from_secs(1);

/// The longest it waits for the budget before looking again, since what's due changes meanwhile.
const LONGEST_PAUSE: Duration = Duration::from_mins(1);

/// Who a read is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    /// Someone is waiting on it, so it goes at once: a client that asked, or an agent whose turn has ended, which may
    /// have committed, pushed or opened a pull request.
    Waiting,
    /// Nobody asked, and it's this workspace's turn, so it waits for the budget.
    InTurn,
}

/// What the hub keeps to read pull requests.
pub(super) struct Reading {
    /// Held while a workspace's pull request is read, so its answers are recorded in the order they were asked for.
    gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<()>>>>,
    /// When each workspace's was last read since the hub started, and how long it goes before a read nobody asks for.
    schedules: Mutex<HashMap<WorkspaceId, Schedule>>,
    budget: Mutex<Budget>,
}

impl Reading {
    pub(super) fn new() -> Self {
        let budget = Mutex::new(Budget::new(Instant::now()));
        Self { gates: Mutex::new(HashMap::new()), schedules: Mutex::new(HashMap::new()), budget }
    }

    /// For one that couldn't be read in turn.
    fn wait_longer(&self, id: &WorkspaceId) {
        let mut schedules = self.schedules.lock();
        let last = schedules.get(id).copied();
        schedules.insert(id.clone(), Schedule::after(last, Instant::now(), Asked::InTurn, Went::Failed));
    }
}

impl Hub {
    /// Reads it now, records it and returns it, for a client that asks. Fails only when nothing is known of it and it
    /// can't be read now.
    pub(crate) async fn workspace_pull_request(
        self: &Arc<Self>,
        id: WorkspaceId,
    ) -> Result<Option<PullRequest>, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        self.read_pull_request(&id, path, &link, Asked::Waiting).await
    }

    /// Reads it through the workspace's worker and records what it finds, one read at a time for each workspace, so an
    /// older answer can't land after a newer one. A read asked for while another waited shares the answer of the one
    /// that began after it was asked. One someone is waiting on spends its calls here, and one in turn already has.
    ///
    /// Says what's known when it can't read, and fails only when nothing is: when GitHub's limit can't spare the
    /// calls, or the worker can't say, such as while GitHub can't be reached. Where it can't be read at all, such as
    /// without `gh`, what's known is kept, and with nothing known it's recorded as none, so the app offers to create
    /// one rather than looking for ever.
    async fn read_pull_request(
        self: &Arc<Self>,
        id: &WorkspaceId,
        path: String,
        link: &WorkerLink,
        asked: Asked,
    ) -> Result<Option<PullRequest>, ApiError> {
        let asked_at = Instant::now();
        let gate = self.pull_requests.gates.lock().entry(id.clone()).or_default().clone();
        let _one_at_a_time = gate.lock().await;
        let known = self.workspace(id).await?.pull_request;
        let last = self.pull_requests.schedules.lock().get(id).copied();
        if last.is_some_and(|last| last.read_at > asked_at) {
            return known.map(|found| found.pull_request).ok_or_else(couldnt_read);
        }
        if asked == Asked::Waiting && self.spend(link, &path, calls_to_read(known.as_ref()), asked).await.is_err() {
            return known.map(|found| found.pull_request).ok_or_else(limit_used_up);
        }
        let read_at = Instant::now();
        self.pull_requests.schedules.lock().insert(id.clone(), Schedule::starting(read_at));
        let read = match link.call(WorkerRequest::PullRequest { path }).await {
            Ok(WorkerResponse::PullRequest { pull_request }) => Ok(PullRequestFound { pull_request }),
            Ok(reply) => Err(unexpected(&reply)),
            Err(e) => Err(e),
        };
        let cant_read = read.as_ref().is_err_and(|e| e.code == ErrorCode::Unavailable);
        let recording = match &read {
            Ok(found) => Some(found.clone()),
            Err(_) if cant_read => Some(known.clone().unwrap_or(PullRequestFound { pull_request: None })),
            Err(_) => None,
        };
        let changed = match recording.clone() {
            Some(found) => {
                let owned = id.clone();
                self.blocking(move |store| store.set_workspace_pull_request(&owned, &found)).await?
            }
            None => None,
        };
        let went = match &read {
            Ok(_) if changed.is_some() => Went::Changed,
            Ok(found) => Went::Same(found.pull_request.as_ref()),
            Err(e) => {
                debug!(%id, cant_read, "the pull request couldn't be read: {}", e.message);
                Went::Failed
            }
        };
        let next = Schedule::after(last, read_at, asked, went);
        self.pull_requests.schedules.lock().insert(id.clone(), next);
        if let Some(workspace) = changed {
            self.notify(HubNotice::Workspace(workspace));
        }
        match recording.or(known) {
            Some(found) => Ok(found.pull_request),
            None => read.map(|found| found.pull_request),
        }
    }

    /// Spends the calls a read takes from the budget, asking GitHub how much of its limit is left first when what it
    /// last said is old, or else says when it's worth trying again.
    async fn spend(&self, link: &WorkerLink, path: &str, calls: u32, asked: Asked) -> Result<(), Instant> {
        if self.pull_requests.budget.lock().needs_github(Instant::now()) {
            let reply = link.call(WorkerRequest::GithubRateLimit { path: path.to_string() }).await;
            let mut budget = self.pull_requests.budget.lock();
            match reply {
                Ok(WorkerResponse::GithubRateLimit(limit)) => budget.github_said(limit, Instant::now(), now_ms()),
                Ok(_) | Err(_) => budget.github_unknown(Instant::now()),
            }
        }
        self.pull_requests.budget.lock().take(calls, asked, Instant::now())
    }

    /// For a workspace just made, whose branch was only now taken from its base, so there's no pull request to find
    /// until an agent's turn makes one, and the turn's end reads it. Until then it waits its turn with the rest.
    pub(super) fn nothing_to_read_yet(&self, id: &WorkspaceId) {
        self.pull_requests.schedules.lock().insert(id.clone(), Schedule::starting(Instant::now()));
    }

    /// After an agent's turn. Only through the worker that's running, as the agent's is, so this never starts one.
    pub(crate) fn read_pull_request_after_turn(self: &Arc<Self>, id: &WorkspaceId) {
        let Some(link) = self.existing_worker(id) else { return };
        let (hub, id) = (self.clone(), id.clone());
        tokio::spawn(async move {
            let Ok(workspace) = hub.workspace(&id).await else { return };
            let Some(path) = workspace.path.filter(|_| workspace.status == WorkspaceStatus::Ready) else { return };
            if let Err(e) = hub.read_pull_request(&id, path, &link, Asked::Waiting).await {
                debug!(%id, "reading the pull request after a turn: {}", e.message);
            }
        });
    }

    /// Reads the pull requests nobody asks for, one at a time, whichever [`schedule::due`] says is next, as the budget
    /// can spare their calls. Not while no client is connected, since nobody would see them. Once one connects
    /// again, whatever has come due meanwhile is read. Runs until the hub stops.
    pub(crate) async fn watch_pull_requests(self: Arc<Self>) {
        let mut pause = PACE;
        loop {
            tokio::time::sleep(pause).await;
            pause = PACE;
            if self.clients.load(Ordering::Relaxed) == 0 {
                continue;
            }
            let Ok(workspaces) = self.workspace_list(false).await else { continue };
            let next = schedule::due(&workspaces, &self.pull_requests.schedules.lock(), Instant::now()).cloned();
            let Some(workspace) = next else { continue };
            if let Err(until) = self.read_in_turn(&workspace).await {
                pause = until.saturating_duration_since(Instant::now()).clamp(PACE, LONGEST_PAUSE);
            }
        }
    }

    /// Reads one nobody asked for, once the budget can spare its calls, starting its workspace's worker if it has
    /// none, and stopping it again after if nothing else has wanted it meanwhile. Says when it's worth trying again
    /// when the budget can't spare them.
    async fn read_in_turn(self: &Arc<Self>, workspace: &Workspace) -> Result<(), Instant> {
        let id = &workspace.id;
        let Some(path) = workspace.path.clone() else {
            // So it doesn't come up again at once, ahead of the rest.
            self.pull_requests.wait_longer(id);
            return Ok(());
        };
        let calls = calls_to_read(workspace.pull_request.as_ref());
        self.pull_requests.budget.lock().allows(calls, Asked::InTurn, Instant::now())?;
        let link = match self.worker_to_read(id).await {
            Ok(link) => link,
            Err(e) => {
                debug!(%id, "starting a worker to read the pull request: {}", e.message);
                self.pull_requests.wait_longer(id);
                return Ok(());
            }
        };
        let spent = self.spend(&link, &path, calls, Asked::InTurn).await;
        if spent.is_ok()
            && let Err(e) = self.read_pull_request(id, path, &link, Asked::InTurn).await
        {
            debug!(%id, "reading the pull request in turn: {}", e.message);
        }
        self.stop_worker_only_read(workspace, &link).await;
        spent
    }
}

fn limit_used_up() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "GitHub's hourly limit is used up, so it can't be read until it resets")
}

fn couldnt_read() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the pull request couldn't be read")
}
