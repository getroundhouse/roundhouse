use super::super::Hub;
use super::own::{WaitsRead, WorkspaceRead};
use super::recording::Found;
use super::schedule;
use super::token_limits::TokenLimits;
use super::unseen::Unseens;
use crate::jobs;
use crate::store::now_ms;
use github::{Leaving, Unread};
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{OwnPullRequest, PullRequestFound, Workspace, WorkspaceId};
use schedule::{Schedule, Went};
use std::collections::HashMap;
use std::sync::Arc;
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
    /// Only while the cache has no read of it that began within [`crate::Timers::fresh_for_clients`], for a client that
    /// asks,
    /// which takes what a read just before it found, as [`Hub::own_from_cache`] says. Never for a read that follows
    /// something that may have changed it, such as a turn ending, which has to begin after it.
    UnlessFresh,
}

/// What a read has to say.
#[derive(Debug)]
pub(super) enum Answer {
    /// What it found, read since it was asked for.
    Read(Option<OwnPullRequest>),
    /// That it couldn't be read now, and why, with what was known before, if anything was.
    Known(Option<PullRequestFound>, ApiError),
}

impl Answer {
    /// For a client, which shows what was known when it can't be read now, and fails only when nothing was.
    pub(super) fn or_known(self) -> Result<Option<OwnPullRequest>, ApiError> {
        match self {
            Self::Read(pull_request) | Self::Known(Some(PullRequestFound { pull_request }), _) => Ok(pull_request),
            Self::Known(None, e) => Err(e),
        }
    }

    /// For an agent, which hears only what was read since it asked.
    pub(super) fn read(self) -> Result<Option<OwnPullRequest>, ApiError> {
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
    /// no worker, and so don't wait behind a workspace's read whose worker is slow to say how its worktree stands.
    pub(super) agent_gates: Mutex<HashMap<WorkspaceId, Arc<AsyncMutex<()>>>>,
    pub(super) remote_read_at: Mutex<HashMap<WorkspaceId, Instant>>,
    /// What GitHub last said of each workspace's repository being unseen.
    pub(super) unseen: Unseens,
}

impl Reading {
    pub(in super::super) fn new() -> Self {
        Self {
            gates: Mutex::new(HashMap::new()),
            schedules: Mutex::new(HashMap::new()),
            limits: TokenLimits::default(),
            agent_gates: Mutex::new(HashMap::new()),
            remote_read_at: Mutex::new(HashMap::new()),
            unseen: Unseens::new(),
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
    /// Reads it now, unless a read just before found it, records it and returns it, for a client that asks. Fails only
    /// when nothing is known of it and it can't be read now.
    pub(crate) async fn workspace_pull_request(
        self: &Arc<Self>,
        id: WorkspaceId,
    ) -> Result<Option<OwnPullRequest>, ApiError> {
        self.read_pull_request(&id, Asked::Waiting, When::UnlessFresh).await?.or_known()
    }

    /// Reads it for an agent, as an [`Asked::Agent`] read, so it fails rather than say what was known before.
    pub(crate) async fn pull_request_for_agent(
        self: &Arc<Self>,
        id: &WorkspaceId,
    ) -> Result<Option<OwnPullRequest>, ApiError> {
        self.read_pull_request(id, Asked::Agent, When::Always).await?.read()
    }

    /// Reads it from GitHub and records what it finds, one read at a time for each workspace, so an older answer can't
    /// land after a newer one. A read asked for while another waited shares the answer of the one that began after it
    /// was asked.
    ///
    /// Says what was known when it can't read: when GitHub's limit for its token can't spare it, or GitHub can't be
    /// reached. Where it can't be read at all, such as with no login to read it as, what's known is kept, and with
    /// nothing known it's recorded as none, so the app offers to create one rather than looking for ever. Where GitHub
    /// shows no repository, a none known before is no longer known, as [`Hub::record`] says. It reads what the
    /// workspace's waits watch too, and they judge what it finds.
    pub(super) async fn read_pull_request(
        self: &Arc<Self>,
        id: &WorkspaceId,
        asked: Asked,
        when: When,
    ) -> Result<Answer, ApiError> {
        let asked_at = Instant::now();
        if asked == Asked::Agent {
            self.pull_requests.space_out(id, self.timers.agent_spacing).await;
        }
        jobs::step("waiting for the workspace's turn to read its pull request");
        let gate = self.pull_requests.gates.lock().entry(id.clone()).or_default().clone();
        let held = gate.lock_owned().await;
        let workspace = self.workspace(id).await?;
        let known = self.known_of(&workspace);
        let last = self.pull_requests.schedules.lock().get(id).copied();
        if let Some(last) = last.filter(|last| last.read_at > asked_at) {
            let answer = match known {
                Some(found) if last.found => Answer::Read(found.pull_request),
                known => Answer::Known(known, couldnt_read()),
            };
            return Ok(answer);
        }
        // What's known is the branch's own, found since the worktree checked it out, unless the cache has forgotten it.
        let forgotten = known.is_none() && workspace.pull_request_link.is_some();
        if when == When::OnAnotherBranch && !workspace.pull_request_found_on_another_branch() && !forgotten {
            return Ok(Answer::Read(known.and_then(|found| found.pull_request)));
        }
        // A client asks, and a read just now, such as as a turn ended, or for another client, found it.
        if when == When::UnlessFresh
            && let Some(found) = self.own_from_cache(&workspace).await
        {
            let (_, changed) = self.record(&workspace, &Ok(found.clone())).await?;
            if let Some(workspace) = changed {
                self.notify(HubNotice::Workspace(workspace));
            }
            return Ok(Answer::Read(found.read.pull_request));
        }
        let read_at = Instant::now();
        // One whose branch needs nothing of GitHub, such as one pushed nowhere yet, needs no token.
        let asking = self.asking(vec![workspace.clone()]).await;
        let read = match self.reads_as() {
            _ if !asking.needs_github() => asking.settled(),
            Err(e) => asking.failed(&super::polling::no_login(&e)),
            Ok(reads_as) => {
                if self.pull_requests.limits.allows(&reads_as, asked, now_ms()).is_err() {
                    return Ok(Answer::Known(known, limit_used_up()));
                }
                match self.token_of(&reads_as) {
                    Err(e) => asking.failed(&e),
                    Ok(token) => {
                        self.pull_requests.schedules.lock().insert(id.clone(), Schedule::starting(read_at));
                        let read = self.read_own(&token, asking, asked).await;
                        self.github_said(&reads_as, read.rate_limit);
                        read.found
                    }
                }
            }
        };
        let read = read.into_iter().next().map_or_else(|| WorkspaceRead::unread(couldnt_read()), |(_, read)| read);
        // However the job this is in goes on, so what's recorded and what's said of it agree.
        let hub = self.clone();
        let taking = Taking { workspace, known, last, read_at, asked, _held: held };
        jobs::uncut(async move { hub.take_answer(taking, read).await }).await
    }

    /// What a read does once GitHub has answered: records which pull request it found, schedules the next read, judges
    /// the waits on what it read and says what changed, its waits first, so no client sees the bar move on while a
    /// wait goes on. Whether it found something new is judged against what was known before it began, since the read
    /// itself keeps what it found in the cache, where what's known is.
    pub(super) async fn take_answer(
        self: Arc<Self>,
        Taking { workspace, known, last, read_at, asked, _held }: Taking,
        WorkspaceRead { own: read, waits }: WorkspaceRead,
    ) -> Result<Answer, ApiError> {
        let id = &workspace.id;
        let (recorded, changed) = self.record(&workspace, &read).await?;
        // Pull requests that moved while they were read were read all the same, so that's no sign GitHub can't be.
        let waits_failed = waits.iter().any(|w| w.read.as_ref().is_err_and(|u| u.error.code != ErrorCode::Conflict));
        let went = match &read {
            Ok(_) if waits_failed => Went::Failed,
            // Every read is newer than the last, so it's what it says that's compared.
            Ok(Found { read, .. }) if !known.as_ref().is_some_and(|known| known.says_the_same(read)) => Went::Changed,
            Ok(Found { read, .. }) => Went::Same(read.pull_request.as_ref()),
            Err(unread) => {
                let e = unread.error();
                debug!(%id, code = ?e.code, "the pull request couldn't be read: {}", e.message);
                Went::Failed
            }
        };
        let next = Schedule::after(last, read_at, asked, went);
        self.pull_requests.schedules.lock().insert(id.clone(), next);
        for WaitsRead { repository, read } in waits {
            match read {
                Ok(snapshot) => self.observe_remote_waits(id, &snapshot).await,
                Err(Unread { error, affected }) => {
                    self.remote_waits_failed(id, Some(&repository), &error, &affected).await;
                }
            }
        }
        // What's known once it's recorded, which is no longer the none known before where GitHub showed no repository.
        let known = changed.as_ref().map_or(known, |changed| self.known_of(changed));
        if let Some(workspace) = changed {
            self.notify(HubNotice::Workspace(workspace));
        }
        // After the workspace, so whoever's told has it as it's recorded now.
        self.keep_unseen(id, &read).await;
        self.github_read_ended(id, read.is_err());
        let answer = match read {
            Ok(Found { read, .. }) => Answer::Read(read.pull_request),
            Err(unread) => Answer::Known(recorded.or(known), unread.error()),
        };
        Ok(answer)
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

/// What taking a read's answer needs besides the answer: the workspace as it was when the read began, what was known
/// of its pull request then, when it was last read and when this read began, why it was read, and the workspace's turn
/// to read, held until its answer is taken.
pub(super) struct Taking {
    pub(super) workspace: Workspace,
    pub(super) known: Option<PullRequestFound>,
    pub(super) last: Option<Schedule>,
    pub(super) read_at: Instant,
    pub(super) asked: Asked,
    pub(super) _held: ReadsHeld,
}

fn limit_used_up() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "GitHub's hourly limit is used up, so it can't be read until it resets")
}

pub(super) fn couldnt_read() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the pull request couldn't be read")
}
