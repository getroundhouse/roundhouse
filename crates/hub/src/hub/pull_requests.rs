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
use protocol::model::{BranchName, PullRequest, PullRequestFound, Workspace, WorkspaceId, WorkspaceStatus};
use protocol::worker::{WorkerRequest, WorkerResponse};
use schedule::{Schedule, Went};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tracing::debug;

use super::{Hub, unexpected};
use crate::store::{Store, now_ms};
use crate::worker_link::WorkerLink;

mod budget;
mod schedule;
#[cfg(test)]
mod tests;

/// How often the hub looks for a pull request to read that nobody asked for, while the budget can spare the calls.
const PACE: Duration = Duration::from_secs(1);

/// The longest it waits for the budget before looking again, since what's due changes meanwhile.
const LONGEST_PAUSE: Duration = Duration::from_mins(1);

/// How long after a workspace's last read an agent asking waits before its own. Long enough that an agent asking in a
/// loop reads at most six times a minute, and short enough that one asking again after it pushes waits little.
const AGENT_SPACING: Duration = Duration::from_secs(10);

/// Who a read is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    /// Someone is waiting on it, so it goes at once: a client that asked, or an agent whose turn has ended, which may
    /// have committed, pushed or opened a pull request.
    Waiting,
    /// An agent asked, through the worker's tools, or its worktree checked out another branch or pushed the one it's
    /// on, which an agent can do over and over. It goes at once, as a read someone waits on does, but no sooner than
    /// [`AGENT_SPACING`] after the last, and leaves the part of GitHub's limit kept for everything else, as a read in
    /// turn does. So an agent asking, switching or pushing in a loop can't spend the limit that the user's own `gh`
    /// and the other agents share. It's never answered with what was known before, which may be from before the agent
    /// pushed.
    Agent,
    /// Nobody asked, and it's this workspace's turn, so it waits for the budget.
    InTurn,
}

/// Whether a read goes ahead whatever's known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum When {
    Always,
    /// Only while what's known was found on another branch than the one checked out, as just after the worktree
    /// checks out another, since a read that began meanwhile may have found the new one's already.
    OnAnotherBranch,
}

/// What a read has to say.
#[derive(Debug)]
enum Answer {
    /// What it found, read since it was asked for.
    Read(Option<PullRequest>),
    /// That it couldn't be read now, and why, with what was known before, if anything was.
    Known(Option<PullRequestFound>, ApiError),
}

impl Answer {
    /// For a client, which shows what was known when it can't be read now, and fails only when nothing was.
    fn or_known(self) -> Result<Option<PullRequest>, ApiError> {
        match self {
            Self::Read(pull_request) | Self::Known(Some(PullRequestFound { pull_request }), _) => Ok(pull_request),
            Self::Known(None, e) => Err(e),
        }
    }

    /// For an agent, which hears only what was read since it asked.
    fn read(self) -> Result<Option<PullRequest>, ApiError> {
        match self {
            Self::Read(pull_request) => Ok(pull_request),
            Self::Known(_, e) => Err(e),
        }
    }
}

/// A workspace's reads, held back while it's kept.
pub(super) type ReadsHeld = OwnedMutexGuard<()>;

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

    /// Until [`AGENT_SPACING`] after the workspace's last read, for an agent asking.
    async fn space_out(&self, id: &WorkspaceId) {
        let last = self.schedules.lock().get(id).map(|last| last.read_at);
        if let Some(last) = last {
            tokio::time::sleep_until((last + AGENT_SPACING).into()).await;
        }
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
        self.read_pull_request(&id, path, &link, Asked::Waiting, When::Always).await?.or_known()
    }

    /// Reads it for an agent, through the worker of the agent that asked, as an [`Asked::Agent`] read, so it fails
    /// rather than say what was known before.
    pub(crate) async fn pull_request_for_agent(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &WorkerLink,
    ) -> Result<Option<PullRequest>, ApiError> {
        let (_, path) = self.ready_path(id).await?;
        self.read_pull_request(id, path, link, Asked::Agent, When::Always).await?.read()
    }

    /// Reads it for an agent starting a wait, as [`Hub::pull_request_for_agent`] does, and holds the workspace's next
    /// read back until what it returns goes, so the wait is recorded before another read can judge the waits.
    pub(super) async fn pull_request_to_wait_on(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &WorkerLink,
    ) -> Result<(Option<PullRequest>, ReadsHeld), ApiError> {
        let (_, path) = self.ready_path(id).await?;
        let (answer, held) = self.read_holding(id, path, link, Asked::Agent, When::Always).await?;
        Ok((answer.read()?, held))
    }

    /// Reads it as [`Hub::read_holding`] does, and lets the next read go.
    async fn read_pull_request(
        self: &Arc<Self>,
        id: &WorkspaceId,
        path: String,
        link: &WorkerLink,
        asked: Asked,
        when: When,
    ) -> Result<Answer, ApiError> {
        Ok(self.read_holding(id, path, link, asked, when).await?.0)
    }

    /// Reads it through the workspace's worker and records what it finds, one read at a time for each workspace, so an
    /// older answer can't land after a newer one. A read asked for while another waited shares the answer of the one
    /// that began after it was asked. One someone is waiting on spends its calls here, and one in turn already has.
    ///
    /// Says what was known when it can't read: when GitHub's limit can't spare the calls, or the worker can't say,
    /// such as while GitHub can't be reached. Where it can't be read at all, such as without `gh`, what's known is
    /// kept, and with nothing known it's recorded as none, so the app offers to create one rather than looking for
    /// ever. A read that finds it judges the waits on it. Returns the workspace's reads held back, for whoever has more
    /// to do before the next.
    async fn read_holding(
        self: &Arc<Self>,
        id: &WorkspaceId,
        path: String,
        link: &WorkerLink,
        asked: Asked,
        when: When,
    ) -> Result<(Answer, ReadsHeld), ApiError> {
        let asked_at = Instant::now();
        if asked == Asked::Agent {
            self.pull_requests.space_out(id).await;
        }
        let gate = self.pull_requests.gates.lock().entry(id.clone()).or_default().clone();
        let held = gate.lock_owned().await;
        let workspace = self.workspace(id).await?;
        let known = workspace.pull_request.clone();
        let last = self.pull_requests.schedules.lock().get(id).copied();
        if let Some(last) = last.filter(|last| last.read_at > asked_at) {
            let answer = match known {
                Some(found) if last.found => Answer::Read(found.pull_request),
                known => Answer::Known(known, couldnt_read()),
            };
            return Ok((answer, held));
        }
        // What's known is the branch's own, found since the worktree checked it out.
        if when == When::OnAnotherBranch && !workspace.pull_request_found_on_another_branch() {
            return Ok((Answer::Read(known.and_then(|found| found.pull_request)), held));
        }
        if asked != Asked::InTurn && self.spend(link, &path, calls_to_read(known.as_ref()), asked).await.is_err() {
            return Ok((Answer::Known(known, limit_used_up()), held));
        }
        let read_at = Instant::now();
        self.pull_requests.schedules.lock().insert(id.clone(), Schedule::starting(read_at));
        let read = ask(link, path).await;
        let (recorded, changed) = self.record(&workspace, &read).await?;
        let went = match &read {
            Ok(_) if changed.is_some() => Went::Changed,
            Ok(Found { found, .. }) => Went::Same(found.pull_request.as_ref()),
            Err(e) => {
                debug!(%id, code = ?e.code, "the pull request couldn't be read: {}", e.message);
                Went::Failed
            }
        };
        let next = Schedule::after(last, read_at, asked, went);
        self.pull_requests.schedules.lock().insert(id.clone(), next);
        if let Ok(Found { found, .. }) = &read {
            self.end_waits(id, found).await;
        }
        if let Some(workspace) = changed {
            self.notify(HubNotice::Workspace(workspace));
        }
        let answer = match read {
            Ok(Found { found, .. }) => Answer::Read(found.pull_request),
            Err(e) => Answer::Known(recorded.or(known), e),
        };
        Ok((answer, held))
    }

    /// Records what a read found, with the branch it found it on, and where it can't be read at all, what's known, or
    /// none when nothing is. Returns what it recorded, and the workspace if that changed it.
    async fn record(
        &self,
        workspace: &Workspace,
        read: &Result<Found, ApiError>,
    ) -> Result<(Option<PullRequestFound>, Option<Workspace>), ApiError> {
        let Found { found, on } = match read {
            Ok(found) => found.clone(),
            // On the branch it was found on, which it still is, since nothing new was found.
            Err(e) if e.code == ErrorCode::Unavailable => Found {
                found: workspace.pull_request.clone().unwrap_or(PullRequestFound { pull_request: None }),
                on: workspace.pull_request_branch.clone(),
            },
            Err(_) => return Ok((None, None)),
        };
        let (id, recording) = (workspace.id.clone(), found.clone());
        let changed =
            self.blocking(move |store| store.set_workspace_pull_request(&id, &recording, on.as_ref())).await?;
        Ok((Some(found), changed))
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
        if let Some(link) = self.existing_worker(id) {
            self.read_pull_request_through(id, link, Asked::Waiting, When::Always);
        }
    }

    /// After the worktree checked out another branch or pushed the one it's on, through the worker that saw it, as
    /// [`Asked::Agent`] says.
    pub(super) fn read_pull_request_after_move(self: &Arc<Self>, id: &WorkspaceId, link: Arc<WorkerLink>, when: When) {
        self.read_pull_request_through(id, link, Asked::Agent, when);
    }

    /// For something that may have changed it, through the worker it happened in.
    fn read_pull_request_through(self: &Arc<Self>, id: &WorkspaceId, link: Arc<WorkerLink>, asked: Asked, when: When) {
        let (hub, id) = (self.clone(), id.clone());
        tokio::spawn(async move {
            let Ok(workspace) = hub.workspace(&id).await else { return };
            let Some(path) = workspace.path.filter(|_| workspace.status == WorkspaceStatus::Ready) else { return };
            let read = hub.read_pull_request(&id, path, &link, asked, when).await;
            if let Err(e) = read.and_then(Answer::or_known) {
                debug!(%id, "reading the pull request as it may have changed: {}", e.message);
            }
        });
    }

    /// Reads the pull requests nobody asks for, one at a time, whichever [`Hub::to_read_in_turn`] says is next, as the
    /// budget can spare their calls. Runs until the hub stops.
    pub(crate) async fn watch_pull_requests(self: Arc<Self>) {
        let mut pause = PACE;
        loop {
            tokio::time::sleep(pause).await;
            pause = PACE;
            let Some(workspace) = self.to_read_in_turn().await else { continue };
            if let Err(until) = self.read_in_turn(&workspace).await {
                pause = until.saturating_duration_since(Instant::now()).clamp(PACE, LONGEST_PAUSE);
            }
        }
    }

    /// The workspace whose pull request is due to be read now that nobody has asked, as [`schedule::due`] says. While
    /// no client is connected, only one an agent waits on, since nobody would see the rest. Once one connects again,
    /// whatever has come due meanwhile is read.
    pub(super) async fn to_read_in_turn(&self) -> Option<Workspace> {
        // A store that fails says why as it does, and those waited on wait their turn with the rest meanwhile.
        let waited = self.blocking(Store::workspaces_waited_on).await.unwrap_or_default();
        let seen = self.clients.load(Ordering::Relaxed) > 0;
        if !seen && waited.is_empty() {
            return None;
        }
        let mut workspaces = self.workspace_list(false).await.ok()?;
        if !seen {
            workspaces.retain(|workspace| waited.contains(&workspace.id));
        }
        schedule::due(&workspaces, &self.pull_requests.schedules.lock(), &waited, Instant::now()).cloned()
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
            && let Err(e) =
                self.read_pull_request(id, path, &link, Asked::InTurn, When::Always).await.and_then(Answer::or_known)
        {
            debug!(%id, "reading the pull request in turn: {}", e.message);
        }
        self.stop_worker_only_read(workspace, &link).await;
        spent
    }
}

/// What a read found, and the branch it found it on.
#[derive(Clone)]
struct Found {
    found: PullRequestFound,
    on: Option<BranchName>,
}

async fn ask(link: &WorkerLink, path: String) -> Result<Found, ApiError> {
    let reply = link.call(WorkerRequest::PullRequest { path }).await?;
    let WorkerResponse::PullRequest { pull_request, branch } = reply else { return Err(unexpected(&reply)) };
    Ok(Found { found: PullRequestFound { pull_request }, on: branch })
}

fn limit_used_up() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "GitHub's hourly limit is used up, so it can't be read until it resets")
}

fn couldnt_read() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the pull request couldn't be read")
}
