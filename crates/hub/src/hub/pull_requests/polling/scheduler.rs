use std::collections::HashMap;
use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::Workspace;
use tokio::time::Instant;
use tracing::debug;

use crate::hub::Followed;
use crate::hub::pull_requests::own::WorkspaceRead;
use crate::hub::pull_requests::reading::{Taking, couldnt_read};
use crate::hub::pull_requests::remote::{ReadsAs, waited};
use crate::hub::pull_requests::{Asked, Hub, schedule};
use crate::jobs::{self, Ended, JobKind, Limits};
use crate::store::{Store, now_ms};
use github::BRANCHES_A_QUERY;

/// The queries a read of workspaces' pull requests makes at most, for every [`BRANCHES_A_QUERY`] of its workspaces and
/// of the pull requests their waits watch, each given the time the hub gives a call to GitHub: one to ask which pull
/// requests their branches have, then two rounds for most, and as many again for pages and for what's under them in
/// their stacks, read again twice for pull requests that moved as they were read.
const QUERIES_AT_MOST: u32 = 13;

impl Hub {
    /// Reads what nobody asks for: workspaces' pull requests, and what clients show as chips, each as it's due. Runs
    /// until the hub stops.
    pub(crate) async fn watch_pull_requests(self: Arc<Self>) {
        tokio::join!(self.clone().watch_own_pull_requests(), self.watch_followed_references());
    }

    /// Starts reading workspaces' pull requests nobody asks for, as jobs, every one that's due as one token together,
    /// as GitHub's limit for each token can spare. It waits on no read, so one that hangs holds up only what's read as
    /// its token, and that only until its limit.
    #[expect(clippy::disallowed_methods, reason = "the pace the pull requests nobody asks for are read at")]
    async fn watch_own_pull_requests(self: Arc<Self>) {
        let pace = self.timers.read_pace;
        loop {
            tokio::time::sleep(pace).await;
            self.start_due_own_reads().await;
        }
    }

    /// The workspaces whose pull requests are due to be read now that nobody has asked, as [`schedule::due`] says,
    /// those first that are due first, of those no read is under way for. While no client follows the hub or a
    /// workspace, only those an agent waits on, since nobody would see the rest, as what's followed says what the hub
    /// keeps current. Once one follows again, whatever has come due meanwhile is read.
    pub(in crate::hub) async fn to_read_in_turn(&self) -> Vec<Workspace> {
        // A store that fails says why as it does, and those waited on wait their turn with the rest meanwhile.
        let waited = self.blocking(Store::workspaces_waited_on).await.unwrap_or_default();
        let shown = self.followers.workspaces();
        let seen = !shown.is_empty() || self.followers.follows(&Followed::Hub);
        if !seen && waited.is_empty() {
            return Vec::new();
        }
        let Ok(mut workspaces) = self.workspace_list(false).await else { return Vec::new() };
        workspaces.retain(|workspace| seen || waited.contains(&workspace.id));
        // One whose pull request the cache has forgotten, as after it was emptied, is known as none until it's read.
        let listed: Vec<_> = workspaces
            .into_iter()
            .map(|workspace| {
                let state = self
                    .known_of(&workspace)
                    .and_then(|found| found.pull_request)
                    .map(|own| own.pull_request.said().state);
                (workspace, state)
            })
            .collect();
        let due = schedule::due(&listed, &self.pull_requests.schedules.lock(), &waited, &shown, Instant::now());
        due.into_iter().cloned().collect()
    }

    /// How many pull requests the waits in `workspaces` watch, as they're asked of GitHub.
    async fn waited_in(&self, workspaces: &[Workspace]) -> usize {
        let mut waited = waited(self.blocking(Store::remote_waits).await.unwrap_or_default());
        let mut theirs = workspaces.iter().filter_map(|workspace| waited.remove(&workspace.id));
        theirs.by_ref().flatten().map(|waited| waited.selection.numbers().len()).sum()
    }

    /// Starts reading the workspaces that are due, a job for each token they're read as, but for a token whose read
    /// is under way, or whose GitHub limit is down to the part kept for everything else.
    async fn start_due_own_reads(self: &Arc<Self>) {
        let listed_at = Instant::now();
        let mut by_token: Vec<(ReadsAs, Vec<Workspace>)> = Vec::new();
        for workspace in self.to_read_in_turn().await {
            let Ok(reads_as) = self.reads_as() else {
                // So it doesn't come up again at once, ahead of the rest.
                self.pull_requests.wait_longer(&workspace.id);
                continue;
            };
            match by_token.iter_mut().find(|(read_as, _)| *read_as == reads_as) {
                Some((_, workspaces)) => workspaces.push(workspace),
                None => by_token.push((reads_as, vec![workspace])),
            }
        }
        let busy = self.own_reads.busy();
        for (reads_as, workspaces) in by_token.into_iter().filter(|(reads_as, _)| !busy.contains(reads_as)) {
            if self.pull_requests.limits.allows(&reads_as, Asked::InTurn, now_ms()).is_err() {
                continue;
            }
            if !self.own_reads.has_room(JobKind::ReadOwnPullRequests) {
                return;
            }
            self.start_own_read(reads_as, workspaces, listed_at).await;
        }
    }

    /// Starts reading `workspaces`, as they were at `listed_at`, as `reads_as`, as a job, within the limits of a read
    /// that makes [`QUERIES_AT_MOST`], as it says. Once it ends, however it ended, each one it didn't finish waits
    /// longer for its next, and says so on its waits.
    async fn start_own_read(self: &Arc<Self>, reads_as: ReadsAs, workspaces: Vec<Workspace>, listed_at: Instant) {
        if workspaces.is_empty() {
            return;
        }
        let looks = workspaces.len().saturating_add(self.waited_in(&workspaces).await);
        let queries = u32::try_from(looks.div_ceil(BRANCHES_A_QUERY)).unwrap_or(u32::MAX);
        let limits = Hub::read_limits(queries.saturating_mul(QUERIES_AT_MOST));
        let before: Vec<_> = {
            let schedules = self.pull_requests.schedules.lock();
            workspaces.iter().map(|workspace| (workspace.id.clone(), schedules.get(&workspace.id).copied())).collect()
        };
        let (hub, token_of) = (self.clone(), reads_as.clone());
        let work = async move { hub.read_own_in_turn(&token_of, workspaces, listed_at).await };
        let hub = self.clone();
        let after = move |ended: Ended| async move {
            // So those it didn't finish don't come up again soon, ahead of the rest.
            if let Some(error) = unfinished(&ended, limits) {
                for (id, before) in before {
                    hub.pull_requests.wait_longer_than(&id, before);
                    hub.remote_waits_failed(&id, None, &error, &[]).await;
                }
            }
        };
        if let Err(not) = self.own_reads.start(JobKind::ReadOwnPullRequests, reads_as, limits, work, after) {
            debug!(?not, "a read of workspaces' pull requests didn't start");
        }
    }

    /// Reads `workspaces`, as they were at `listed_at`, as `reads_as`, together, and records what each found, but for
    /// one whose read someone asked for has begun since, which records its own. Takes the token first, if any of them
    /// needs GitHub, then each workspace's turn, reading what its record says then.
    async fn read_own_in_turn(self: &Arc<Self>, reads_as: &ReadsAs, workspaces: Vec<Workspace>, listed_at: Instant) {
        let token = self.asking(workspaces.clone()).await.first_asked().map(|_| self.token_of(reads_as));
        let mut reading = Vec::new();
        for listed in workspaces {
            let gate = self.pull_requests.gates.lock().entry(listed.id.clone()).or_default().clone();
            let Ok(held) = gate.try_lock_owned() else { continue };
            let schedule = self.pull_requests.schedules.lock().get(&listed.id).copied();
            if schedule.is_some_and(|schedule| schedule.read_at > listed_at) {
                continue;
            }
            match self.workspace(&listed.id).await {
                // What's known before the read, which keeps what it finds where that's known.
                Ok(workspace) => reading.push((self.known_of(&workspace), workspace, held)),
                Err(e) => {
                    debug!(id = %listed.id, "reading a workspace's record to read its pull request: {}", e.message);
                }
            }
        }
        let read_at = Instant::now();
        let last: HashMap<_, _> = {
            let mut schedules = self.pull_requests.schedules.lock();
            reading
                .iter()
                .map(|(_, workspace, _)| {
                    let last = schedules.insert(workspace.id.clone(), schedule::Schedule::starting(read_at));
                    (workspace.id.clone(), last)
                })
                .collect()
        };
        let asking = self.asking(reading.iter().map(|(_, workspace, _)| workspace.clone()).collect()).await;
        let read = match token {
            Some(Ok(token)) => {
                let read = self.read_own(&token, asking, Asked::InTurn).await;
                self.github_said(reads_as, read.rate_limit);
                read.found
            }
            Some(Err(e)) => {
                debug!(%reads_as, "no token to read workspaces' pull requests with: {}", e.message);
                asking.failed(&e)
            }
            // One pushed since it was listed waits for the next read, rather than this one fetching a token while it
            // holds every workspace's turn.
            None => asking.settled(),
        };
        let mut found: HashMap<_, _> = read.into_iter().collect();
        let hub = self.clone();
        jobs::uncut(async move {
            for (known, workspace, held) in reading {
                let answer = found.remove(&workspace.id).unwrap_or_else(|| WorkspaceRead::unread(couldnt_read()));
                let last = last.get(&workspace.id).copied().flatten();
                let taking = Taking { workspace, known, last, read_at, asked: Asked::InTurn, _held: held };
                if let Err(e) = hub.clone().take_answer(taking, answer).await {
                    debug!("recording a workspace's pull request read in turn: {}", e.message);
                }
            }
        })
        .await;
    }
}

/// `error`, said of a workspace's own pull request where there's no login to read it as: no default account, or one
/// that's blocked. Asking again won't help until someone logs in, so it can't be read at
/// all, and one with nothing known is recorded as none, so the app offers to create one rather than look for ever.
/// A read of waits takes the same as one that may work later instead, so the waits go on waiting.
pub(in crate::hub::pull_requests) fn no_login(error: &ApiError) -> ApiError {
    ApiError::new(ErrorCode::Unavailable, error.message.clone())
}

/// What a read that didn't finish is recorded as on its workspaces' waits, like a read that failed, or `None` for one
/// that did, or that got as far as recording what it read. Their agents read it, so it says what happened in their
/// terms, and the log says where the read had got to.
fn unfinished(ended: &Ended, limits: Limits) -> Option<ApiError> {
    if ended.finished_or_ran_uncut() {
        return None;
    }
    let message = if let Ended::Panicked { .. } = ended {
        "reading the pull requests failed in the hub".to_string()
    } else {
        format!("reading the pull requests took longer than {} s, so it was stopped", limits.work.as_secs())
    };
    Some(ApiError::new(ErrorCode::Worker, message))
}
