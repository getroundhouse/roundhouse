use super::super::Hub;
use super::{budget, schedule};
use crate::store::now_ms;
use crate::worker_link::WorkerLink;
use budget::{Budget, calls_to_read};
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{BranchName, PullRequest, PullRequestFound, Workspace, WorkspaceId};
use protocol::worker;
use protocol::worker::{GithubRateLimit, PullRequestReply};
use schedule::{Schedule, Went};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tracing::debug;

/// How long after a workspace's last read an agent asking waits before its own. Long enough that an agent asking in a
/// loop reads at most six times a minute, and short enough that one asking again after it pushes waits little.
pub(super) const AGENT_SPACING: Duration = Duration::from_secs(10);

/// Who a read is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Asked {
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
pub(in super::super) enum When {
    Always,
    /// Only while what's known was found on another branch than the one checked out, as just after the worktree
    /// checks out another, since a read that began meanwhile may have found the new one's already.
    OnAnotherBranch,
}

/// What a read has to say.
#[derive(Debug)]
pub(super) enum Answer {
    /// What it found, read since it was asked for.
    Read(Option<PullRequest>),
    /// That it couldn't be read now, and why, with what was known before, if anything was.
    Known(Option<PullRequestFound>, ApiError),
}

impl Answer {
    /// For a client, which shows what was known when it can't be read now, and fails only when nothing was.
    pub(super) fn or_known(self) -> Result<Option<PullRequest>, ApiError> {
        match self {
            Self::Read(pull_request) | Self::Known(Some(PullRequestFound { pull_request }), _) => Ok(pull_request),
            Self::Known(None, e) => Err(e),
        }
    }

    /// For an agent, which hears only what was read since it asked.
    pub(super) fn read(self) -> Result<Option<PullRequest>, ApiError> {
        match self {
            Self::Read(pull_request) => Ok(pull_request),
            Self::Known(_, e) => Err(e),
        }
    }
}

/// A workspace's reads, held back while it's kept.
pub(in super::super) type ReadsHeld = OwnedMutexGuard<()>;

/// What the hub keeps to read pull requests.
pub(in super::super) struct Reading {
    /// Held while a workspace's pull request is read, so its answers are recorded in the order they were asked for.
    pub(super) gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<()>>>>,
    /// When each workspace's was last read since the hub started, and how long it goes before a read nobody asks for.
    pub(super) schedules: Mutex<HashMap<WorkspaceId, Schedule>>,
    pub(super) budget: Mutex<Budget>,
    pub(super) remote_read_at: Mutex<HashMap<WorkspaceId, Instant>>,
    pub(super) remote: Mutex<HashMap<super::remote::GroupKey, super::remote::RemoteSchedule>>,
}

impl Reading {
    pub(in super::super) fn new() -> Self {
        let budget = Mutex::new(Budget::new(Instant::now()));
        Self {
            gates: Mutex::new(HashMap::new()),
            schedules: Mutex::new(HashMap::new()),
            budget,
            remote_read_at: Mutex::new(HashMap::new()),
            remote: Mutex::new(HashMap::new()),
        }
    }

    /// For one that couldn't be read in turn.
    pub(super) fn wait_longer(&self, id: &WorkspaceId) {
        let mut schedules = self.schedules.lock();
        let last = schedules.get(id).copied();
        schedules.insert(id.clone(), Schedule::after(last, Instant::now(), Asked::InTurn, Went::Failed));
    }

    /// Until [`AGENT_SPACING`] after the workspace's last read, for an agent asking.
    pub(super) async fn space_out(&self, id: &WorkspaceId) {
        let last =
            self.schedules.lock().get(id).map(|last| last.read_at).max(self.remote_read_at.lock().get(id).copied());
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
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        self.read_pull_request(&id, path, worktree.link(), Asked::Waiting, When::Always).await?.or_known()
    }

    /// Reads it for an agent, through the worker of the agent that asked, as an [`Asked::Agent`] read, so it fails
    /// rather than say what was known before.
    pub(crate) async fn pull_request_for_agent(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &WorkerLink,
    ) -> Result<Option<PullRequest>, ApiError> {
        let path = self.ready_workspace(id).await?.path().to_owned();
        self.read_pull_request(id, path, link, Asked::Agent, When::Always).await?.read()
    }

    /// Reads it for an agent starting a wait, as [`Hub::pull_request_for_agent`] does, and holds the workspace's next
    /// read back until what it returns goes, so the wait is recorded before another read can judge the waits.
    pub(in super::super) async fn pull_request_to_wait_on(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &WorkerLink,
    ) -> Result<(Option<PullRequest>, ReadsHeld), ApiError> {
        let path = self.ready_workspace(id).await?.path().to_owned();
        let (answer, held) = self.read_holding(id, path, link, Asked::Agent, When::Always).await?;
        Ok((answer.read()?, held))
    }

    /// Reads it as [`Hub::read_holding`] does, and lets the next read go.
    pub(super) async fn read_pull_request(
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
    pub(super) async fn read_holding(
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
    pub(super) async fn record(
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
    pub(super) async fn spend(&self, link: &WorkerLink, path: &str, calls: u32, asked: Asked) -> Result<(), Instant> {
        if self.pull_requests.budget.lock().needs_github(Instant::now()) {
            let reply = link.call(GithubRateLimit { path: path.to_string() }).await;
            let mut budget = self.pull_requests.budget.lock();
            match reply {
                Ok(limit) => budget.github_said(limit, Instant::now(), now_ms()),
                Err(_) => budget.github_unknown(Instant::now()),
            }
        }
        self.pull_requests.budget.lock().take(calls, asked, Instant::now())
    }

    /// For a workspace just made, whose branch was only now taken from its base, so there's no pull request to find
    /// until an agent's turn makes one, and the turn's end reads it. Until then it waits its turn with the rest.
    pub(in super::super) fn nothing_to_read_yet(&self, id: &WorkspaceId) {
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
    pub(in super::super) fn read_pull_request_after_move(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: Arc<WorkerLink>,
        when: When,
    ) {
        self.read_pull_request_through(id, link, Asked::Agent, when);
    }

    /// For something that may have changed it, through the worker it happened in.
    pub(super) fn read_pull_request_through(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: Arc<WorkerLink>,
        asked: Asked,
        when: When,
    ) {
        let (hub, id) = (self.clone(), id.clone());
        tokio::spawn(async move {
            let Ok(ready) = hub.ready_workspace(&id).await else { return };
            let path = ready.path().to_owned();
            let read = hub.read_pull_request(&id, path, &link, asked, when).await;
            if let Err(e) = read.and_then(Answer::or_known) {
                debug!(%id, "reading the pull request as it may have changed: {}", e.message);
            }
        });
    }
}

/// What a read found, and the branch it found it on.
#[derive(Clone)]
pub(super) struct Found {
    pub(super) found: PullRequestFound,
    pub(super) on: Option<BranchName>,
}

async fn ask(link: &WorkerLink, path: String) -> Result<Found, ApiError> {
    let PullRequestReply { pull_request, branch } = link.call(worker::PullRequest { path }).await?;
    Ok(Found { found: PullRequestFound { pull_request }, on: branch })
}

fn limit_used_up() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "GitHub's hourly limit is used up, so it can't be read until it resets")
}

fn couldnt_read() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the pull request couldn't be read")
}
