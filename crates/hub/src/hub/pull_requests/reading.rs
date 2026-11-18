use super::super::Hub;
use super::own::{WaitsRead, WorkspaceRead};
use super::schedule;
use super::token_limits::TokenLimits;
use crate::github::{Leaving, Unread};
use crate::jobs;
use crate::store::now_ms;
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{BranchName, PullRequest, PullRequestFound, Workspace, WorkspaceId};
use schedule::{Schedule, Went};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::Instant;
use tracing::debug;

/// Who a read is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::hub::pull_requests) enum Asked {
    /// Someone is waiting on it, so it goes at once: a client that asked, or an agent whose turn has ended, which may
    /// have committed, pushed or opened a pull request.
    Waiting,
    /// An agent asked, through the worker's tools, or its worktree checked out another branch or pushed the one it's
    /// on, which an agent can do over and over. It goes at once, as a read someone waits on does, but no sooner than
    /// [`crate::Timers::agent_spacing`] after the last, and leaves the part of GitHub's limit kept for everything else,
    /// as a read in turn does. So an agent asking, switching or pushing in a loop can't spend the limit that the user's
    /// own `gh` and the other agents share. It's never answered with what was known before, which may be from before
    /// the agent pushed.
    Agent,
    /// Nobody asked, and it's this workspace's turn, so it leaves the part of GitHub's limit kept for everything else.
    InTurn,
}

impl Asked {
    /// What it leaves of GitHub's limit for its token.
    pub(in crate::hub::pull_requests) fn leaving(self) -> Leaving {
        match self {
            Self::Waiting => Leaving::Nothing,
            Self::Agent | Self::InTurn => Leaving::ForOthers,
        }
    }
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
    /// What GitHub last said is left of its limit for each token the hub reads as.
    pub(super) limits: TokenLimits,
    /// Held while an agent's read of pull requests from GitHub, and a wait it starts, go, so the wait is recorded
    /// before another of the workspace's agents' reads can judge it. Apart from [`Reading::gates`], since these need
    /// no worker, and so don't wait behind one stuck in `gh`.
    pub(super) agent_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<()>>>>,
    pub(super) remote_read_at: Mutex<HashMap<WorkspaceId, Instant>>,
}

impl Reading {
    pub(in super::super) fn new() -> Self {
        Self {
            gates: Mutex::new(HashMap::new()),
            schedules: Mutex::new(HashMap::new()),
            limits: TokenLimits::default(),
            agent_gates: Mutex::new(HashMap::new()),
            remote_read_at: Mutex::new(HashMap::new()),
        }
    }

    /// For one that couldn't be read in turn.
    pub(super) fn wait_longer(&self, id: &WorkspaceId) {
        let last = self.schedules.lock().get(id).copied();
        self.wait_longer_than(id, last);
    }

    /// The same, going by `before`, the schedule after the read before it.
    pub(super) fn wait_longer_than(&self, id: &WorkspaceId, before: Option<Schedule>) {
        let next = Schedule::after(before, Instant::now(), Asked::InTurn, Went::Failed);
        self.schedules.lock().insert(id.clone(), next);
    }

    /// Until `spacing` after the workspace's last read, for an agent asking.
    #[expect(
        clippy::disallowed_methods,
        reason = "the spacing reads of a workspace's pull request that agents ask for keep to"
    )]
    pub(super) async fn space_out(&self, id: &WorkspaceId, spacing: Duration) {
        let last =
            self.schedules.lock().get(id).map(|last| last.read_at).max(self.remote_read_at.lock().get(id).copied());
        if let Some(last) = last {
            tokio::time::sleep_until(last + spacing).await;
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
        self.read_pull_request(&id, Asked::Waiting, When::Always).await?.or_known()
    }

    /// Reads it for an agent, as an [`Asked::Agent`] read, so it fails rather than say what was known before.
    pub(crate) async fn pull_request_for_agent(
        self: &Arc<Self>,
        id: &WorkspaceId,
    ) -> Result<Option<PullRequest>, ApiError> {
        self.read_pull_request(id, Asked::Agent, When::Always).await?.read()
    }

    /// Reads it for an agent starting a wait, as [`Hub::pull_request_for_agent`] does, and holds the workspace's next
    /// read back until what it returns goes, so the wait is recorded before another read can judge the waits.
    pub(in super::super) async fn pull_request_to_wait_on(
        self: &Arc<Self>,
        id: &WorkspaceId,
    ) -> Result<(Option<PullRequest>, ReadsHeld), ApiError> {
        let (answer, held) = self.read_holding(id, Asked::Agent, When::Always).await?;
        Ok((answer.read()?, held))
    }

    /// Reads it as [`Hub::read_holding`] does, and lets the next read go.
    pub(super) async fn read_pull_request(
        self: &Arc<Self>,
        id: &WorkspaceId,
        asked: Asked,
        when: When,
    ) -> Result<Answer, ApiError> {
        Ok(self.read_holding(id, asked, when).await?.0)
    }

    /// Reads it from GitHub and records what it finds, one read at a time for each workspace, so an older answer can't
    /// land after a newer one. A read asked for while another waited shares the answer of the one that began after it
    /// was asked.
    ///
    /// Says what was known when it can't read: when GitHub's limit for its token can't spare it, or GitHub can't be
    /// reached. Where it can't be read at all, such as with no login to read it as, what's known is kept, and with
    /// nothing known it's recorded as none, so the app offers to create one rather than looking for ever. A read that
    /// finds it judges the waits on it. Returns the workspace's reads held back, for whoever has more to do before the
    /// next.
    pub(super) async fn read_holding(
        self: &Arc<Self>,
        id: &WorkspaceId,
        asked: Asked,
        when: When,
    ) -> Result<(Answer, ReadsHeld), ApiError> {
        let asked_at = Instant::now();
        if asked == Asked::Agent {
            self.pull_requests.space_out(id, self.timers.agent_spacing).await;
        }
        jobs::step("waiting for the workspace's turn to read its pull request");
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
        let read_at = Instant::now();
        // One whose branch needs nothing of GitHub, such as one pushed nowhere yet, needs no token.
        let asking = self.asking(vec![workspace.clone()]).await;
        let took_worker = AtomicBool::new(false);
        let read = match self.reads_as(&workspace) {
            _ if !asking.needs_github() => asking.settled(),
            Err(e) => asking.failed(&super::polling::no_login(&e)),
            Ok(reads_as) => {
                if self.pull_requests.limits.allows(&reads_as, asked, now_ms()).is_err() {
                    return Ok((Answer::Known(known, limit_used_up()), held));
                }
                match self.token_for_own_reads(&reads_as, id, &took_worker).await {
                    Err(e) => asking.failed(&e),
                    Ok(token) => {
                        self.pull_requests.schedules.lock().insert(id.clone(), Schedule::starting(read_at));
                        let read = self.read_own(&token, asking, asked).await;
                        self.github_said(&reads_as, read.rate_limit, read.refused);
                        read.found
                    }
                }
            }
        };
        let read = read.into_iter().next().map_or_else(|| WorkspaceRead::unread(couldnt_read()), |(_, read)| read);
        // However the job this is in goes on, so what's recorded and what's said of it agree.
        let (hub, took_worker) = (self.clone(), took_worker.into_inner());
        jobs::uncut(async move {
            let id = workspace.id.clone();
            let answered = hub.clone().take_answer(workspace, read, last, read_at, asked, held).await;
            if took_worker {
                hub.let_go_after_read(&id).await;
            }
            answered
        })
        .await
    }

    /// What a read does once GitHub has answered: records what it found, schedules the next read, judges the waits on
    /// what it read and says what changed, its waits first, so no client sees the bar move on while a wait goes on.
    pub(super) async fn take_answer(
        self: Arc<Self>,
        workspace: Workspace,
        WorkspaceRead { own: read, waits }: WorkspaceRead,
        last: Option<Schedule>,
        read_at: Instant,
        asked: Asked,
        held: ReadsHeld,
    ) -> Result<(Answer, ReadsHeld), ApiError> {
        let (id, known) = (&workspace.id, workspace.pull_request.clone());
        let (recorded, changed) = self.record(&workspace, &read).await?;
        // Pull requests that moved while they were read were read all the same, so that's no sign GitHub can't be.
        let waits_failed = waits.iter().any(|w| w.read.as_ref().is_err_and(|u| u.error.code != ErrorCode::Conflict));
        let went = match &read {
            Ok(_) if waits_failed => Went::Failed,
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
        for WaitsRead { repository, read } in waits {
            match read {
                Ok(snapshot) => self.observe_remote_waits(id, &snapshot).await,
                Err(Unread { error, affected }) => {
                    self.remote_waits_failed(id, Some(&repository), &error, &affected).await;
                }
            }
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
            Err(e) if e.code == ErrorCode::Unavailable => match workspace.pull_request.clone() {
                Some(found) => Found { found, on: workspace.pull_request_branch.clone() },
                None => Found::nothing(workspace.pull_request_branch.clone()),
            },
            Err(_) => return Ok((None, None)),
        };
        let (id, recording) = (workspace.id.clone(), found.clone());
        let changed =
            self.blocking(move |store| store.set_workspace_pull_request(&id, &recording, on.as_ref())).await?;
        Ok((Some(found), changed))
    }

    /// For a workspace just made, whose branch was only now taken from its base, so there's no pull request to find
    /// until an agent's turn makes one, and the turn's end reads it. Until then it waits its turn with the rest.
    pub(in super::super) fn nothing_to_read_yet(&self, id: &WorkspaceId) {
        self.pull_requests.schedules.lock().insert(id.clone(), Schedule::starting(Instant::now()));
    }

    /// After an agent's turn, which may have committed, pushed or opened a pull request.
    pub(crate) fn read_pull_request_after_turn(self: &Arc<Self>, id: &WorkspaceId) {
        self.read_pull_request_soon(id, Asked::Waiting, When::Always);
    }

    /// After the worktree checked out another branch or pushed the one it's on, as [`Asked::Agent`] says.
    pub(in super::super) fn read_pull_request_after_move(self: &Arc<Self>, id: &WorkspaceId, when: When) {
        self.read_pull_request_soon(id, Asked::Agent, when);
    }

    /// For something that may have changed it, on its own, so whatever saw it doesn't wait on GitHub.
    fn read_pull_request_soon(self: &Arc<Self>, id: &WorkspaceId, asked: Asked, when: When) {
        let (hub, id) = (self.clone(), id.clone());
        tokio::spawn(async move {
            let read = hub.read_pull_request(&id, asked, when).await;
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

impl Found {
    /// None, on `on`, as for a branch pushed nowhere.
    pub(super) fn nothing(on: Option<BranchName>) -> Self {
        Self { found: PullRequestFound { pull_request: None }, on }
    }
}

fn limit_used_up() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "GitHub's hourly limit is used up, so it can't be read until it resets")
}

pub(super) fn couldnt_read() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the pull request couldn't be read")
}
