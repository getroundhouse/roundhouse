use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use protocol::client::ApiError;
use protocol::model::{GithubToken, Workspace, WorkspaceId};
use tokio::time::Instant;
use tracing::debug;

use crate::hub::pull_requests::remote::ReadsAs;
use crate::hub::pull_requests::{Asked, Hub, schedule};
use crate::jobs::{self, Ended, JobKind};
use crate::store::{Store, now_ms};

/// The queries a read of workspaces' pull requests makes at most, for every [`crate::github::HeadsAsk`] query its
/// first round makes: that one, then each pull request's two rounds, as many again for pages and for what's under
/// them in their stacks, and read again twice for pull requests that moved as they were read.
const QUERIES_AT_MOST: u32 = 12;

/// The workspaces a query of branches asks after at most, as [`crate::github`] asks them.
const ASKS_A_QUERY: usize = 50;

impl Hub {
    /// Starts reading the pull requests nobody asks for, as jobs, every one that's due as one token together, as
    /// GitHub's limit for each token can spare. It waits on no read, so one that hangs holds up only what's read as
    /// its token, and that only until its limit. Runs until the hub stops.
    #[expect(clippy::disallowed_methods, reason = "the pace the pull requests nobody asks for are read at")]
    pub(crate) async fn watch_pull_requests(self: Arc<Self>) {
        let pace = self.timers.read_pace;
        loop {
            tokio::time::sleep(pace).await;
            // A wait's first, since an agent waits on it.
            self.start_due_wait_reads().await;
            self.start_due_own_reads().await;
        }
    }

    /// The workspaces whose pull requests are due to be read now that nobody has asked, as [`schedule::due`] says,
    /// those first that are due first, of those no read is under way for. While no client is connected, only those an
    /// agent waits on, since nobody would see the rest. Once one connects again, whatever has come due meanwhile is
    /// read.
    pub(in crate::hub) async fn to_read_in_turn(&self) -> Vec<Workspace> {
        // A store that fails says why as it does, and those waited on wait their turn with the rest meanwhile.
        let waited = self.blocking(Store::workspaces_waited_on).await.unwrap_or_default();
        let seen = self.clients.load(Ordering::Relaxed) > 0;
        if !seen && waited.is_empty() {
            return Vec::new();
        }
        let Ok(mut workspaces) = self.workspace_list(false).await else { return Vec::new() };
        workspaces.retain(|workspace| seen || waited.contains(&workspace.id));
        let due = schedule::due(&workspaces, &self.pull_requests.schedules.lock(), &waited, Instant::now());
        due.into_iter().cloned().collect()
    }

    /// Starts reading the workspaces that are due, a job for each token they're read as, but for a token whose read
    /// is under way, or whose GitHub limit is down to the part kept for everything else.
    async fn start_due_own_reads(self: &Arc<Self>) {
        let mut by_token: Vec<(ReadsAs, Vec<Workspace>)> = Vec::new();
        for workspace in self.to_read_in_turn().await {
            let Ok(reads_as) = self.reads_as(&workspace) else {
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
            self.start_own_read(reads_as, workspaces).await;
        }
    }

    /// Starts reading `workspaces` as `reads_as`, as a job, within the limits of a read that makes
    /// [`QUERIES_AT_MOST`] for each query of branches it makes, and may start the first workspace's worker for its
    /// machines' `gh`'s token. Once it ends, however it ended, each one it didn't finish waits longer for its next, and
    /// a worker it took is let go of, unless another read holds it.
    async fn start_own_read(self: &Arc<Self>, reads_as: ReadsAs, workspaces: Vec<Workspace>) {
        let Some(first) = workspaces.first().map(|workspace| workspace.id.clone()) else { return };
        let queries = u32::try_from(workspaces.len().div_ceil(ASKS_A_QUERY)).unwrap_or(u32::MAX);
        let limits = self.wait_read_limits(&first, queries.saturating_mul(QUERIES_AT_MOST)).await;
        let before: Vec<_> = {
            let schedules = self.pull_requests.schedules.lock();
            workspaces.iter().map(|workspace| (workspace.id.clone(), schedules.get(&workspace.id).copied())).collect()
        };
        let took_worker = Arc::new(AtomicBool::new(false));
        let (hub, token_of, took) = (self.clone(), reads_as.clone(), took_worker.clone());
        let work = async move { hub.read_own_in_turn(&token_of, workspaces, &took).await };
        let hub = self.clone();
        let after = move |ended: Ended| async move {
            // So those it didn't finish don't come up again soon, ahead of the rest.
            if !ended.finished_or_ran_uncut() {
                for (id, before) in before {
                    hub.pull_requests.wait_longer_than(&id, before);
                }
            }
            if took_worker.load(Ordering::Relaxed) {
                hub.let_go_after_read(&first).await;
            }
        };
        if let Err(not) = self.own_reads.start(JobKind::ReadOwnPullRequests, reads_as, limits, work, after) {
            debug!(?not, "a read of workspaces' pull requests didn't start");
        }
    }

    /// Reads `workspaces` as `reads_as`, together, and records what each found, but for one whose read someone asked
    /// for is under way, which records its own. Fetches the token only if one needs GitHub. `took` says whether it
    /// took a worker, for the token.
    async fn read_own_in_turn(self: &Arc<Self>, reads_as: &ReadsAs, workspaces: Vec<Workspace>, took: &AtomicBool) {
        let mut reading = Vec::new();
        for workspace in workspaces {
            let gate = self.pull_requests.gates.lock().entry(workspace.id.clone()).or_default().clone();
            if let Ok(held) = gate.try_lock_owned() {
                reading.push((workspace, held));
            }
        }
        let read_at = Instant::now();
        let last: HashMap<_, _> = {
            let mut schedules = self.pull_requests.schedules.lock();
            reading
                .iter()
                .map(|(workspace, _)| {
                    let last = schedules.insert(workspace.id.clone(), schedule::Schedule::starting(read_at));
                    (workspace.id.clone(), last)
                })
                .collect()
        };
        let asking = self.asking(reading.iter().map(|(workspace, _)| workspace.clone()).collect()).await;
        let read = match reading.first().filter(|_| asking.needs_github()) {
            None => asking.settled(),
            Some((first, _)) => match self.token_for_own_reads(reads_as, &first.id, took).await {
                Ok(token) => {
                    let read = self.read_own(&token, asking, Asked::InTurn).await;
                    self.pull_requests.limits.said(reads_as, read.rate_limit);
                    if read.refused {
                        self.reading_token_refused(reads_as);
                    }
                    read
                }
                Err(e) => {
                    debug!(%reads_as, "no token to read workspaces' pull requests with: {}", e.message);
                    asking.failed(&e)
                }
            },
        };
        let mut found: HashMap<_, _> = read.found.into_iter().collect();
        let hub = self.clone();
        jobs::uncut(async move {
            for (workspace, held) in reading {
                let answer = found.remove(&workspace.id).unwrap_or_else(|| Err(super::super::reading::couldnt_read()));
                let last = last.get(&workspace.id).copied().flatten();
                if let Err(e) = hub.clone().take_answer(workspace, answer, last, read_at, Asked::InTurn, held).await {
                    debug!("recording a workspace's pull request read in turn: {}", e.message);
                }
            }
        })
        .await;
    }

    /// The token to read workspaces' pull requests as `reads_as`, for which `first`'s worker is asked for its
    /// machine's `gh`'s, if the hub wasn't handed it, and started for it if it isn't running, as `took` says.
    pub(in crate::hub::pull_requests) async fn token_for_own_reads(
        self: &Arc<Self>,
        reads_as: &ReadsAs,
        first: &WorkspaceId,
        took: &AtomicBool,
    ) -> Result<GithubToken, ApiError> {
        match reads_as {
            ReadsAs::MachineOwn(provider) => self.machine_token_for_waits(provider, first, took).await,
            ReadsAs::Account(_) => self.token_of(reads_as, std::time::Duration::ZERO).await,
        }
    }
}
