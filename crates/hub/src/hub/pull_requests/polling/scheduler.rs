use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use protocol::model::{Workspace, WorkspaceId};
use protocol::worker::GH_RUNS_WITHIN;
use tokio::time::Instant;
use tracing::debug;

use crate::hub::pull_requests::{Answer, Asked, Hub, When, calls_to_read, schedule};
use crate::hub::ready::ReadyWorkspace;
use crate::jobs::{Ended, JobKind, Limits};
use crate::store::Store;
use crate::worker_link::STOP_GRACE;

/// The longest it waits for the budget before looking again, since what's due changes meanwhile.
const LONGEST_PAUSE: Duration = Duration::from_mins(1);

/// What a read nobody asked for takes besides its runs of `gh`: its turn behind a read of the workspace someone asked
/// for, which can take as long again, the git it runs, and the store. So one that reaches the limit made of this and
/// its runs is stuck, not slow.
const BESIDES_GH: Duration = Duration::from_mins(2);

/// The runs of `gh` reading a workspace's pull request makes at most: asking how much of GitHub's limit is left, and
/// the four a read makes when the pull request `gh` finds by the branch's name isn't the one the branch is linked to.
pub(super) const IN_TURN_RUNS: u32 = 5;

impl Hub {
    /// Starts reading the pull requests nobody asks for, as jobs, whichever of each kind is due next, as the budget can
    /// spare their calls. It waits on no read, so one that hangs holds up only what can't run beside it, and that only
    /// until its limit. Runs until the hub stops.
    #[expect(clippy::disallowed_methods, reason = "the pace the pull requests nobody asks for are read at")]
    pub(crate) async fn watch_pull_requests(self: Arc<Self>) {
        let pace = self.timers.read_pace;
        let mut in_turn_after = Instant::now();
        loop {
            tokio::time::sleep(pace).await;
            // A wait's first, since an agent waits on it.
            self.start_due_wait_read().await;
            let now = Instant::now();
            if now >= in_turn_after
                && let Err(until) = self.start_due_read_in_turn().await
            {
                in_turn_after = now + until.saturating_duration_since(now).clamp(pace, LONGEST_PAUSE);
            }
        }
    }

    /// The workspace whose pull request is due to be read now that nobody has asked, as [`schedule::due`] says, of
    /// those no job is reading. While no client is connected, only one an agent waits on, since nobody would see the
    /// rest. Once one connects again, whatever has come due meanwhile is read.
    pub(in crate::hub) async fn to_read_in_turn(&self) -> Option<Workspace> {
        // A store that fails says why as it does, and those waited on wait their turn with the rest meanwhile.
        let waited = self.blocking(Store::workspaces_waited_on).await.unwrap_or_default();
        let seen = self.clients.load(Ordering::Relaxed) > 0;
        if !seen && waited.is_empty() {
            return None;
        }
        let mut workspaces = self.workspace_list(false).await.ok()?;
        let busy = self.jobs.busy();
        workspaces.retain(|workspace| (seen || waited.contains(&workspace.id)) && !busy.contains(&workspace.id));
        schedule::due(&workspaces, &self.pull_requests.schedules.lock(), &waited, Instant::now()).cloned()
    }

    /// Starts reading the one that's due, as a job, once the budget can spare its calls, or says when it's worth
    /// looking again.
    async fn start_due_read_in_turn(self: &Arc<Self>) -> Result<(), Instant> {
        if !self.jobs.has_room(JobKind::ReadPullRequestInTurn) {
            return Ok(());
        }
        let Some(workspace) = self.to_read_in_turn().await else { return Ok(()) };
        let id = workspace.id.clone();
        let Ok(path) = ReadyWorkspace::new(workspace.clone()).map(|ready| ready.path().to_owned()) else {
            // So it doesn't come up again at once, ahead of the rest.
            self.pull_requests.wait_longer(&id);
            return Ok(());
        };
        let calls = calls_to_read(workspace.pull_request.as_ref());
        // Only asked here, since they're spent once the worker has asked GitHub how much of its limit is left.
        self.pull_requests.budget.lock().allows(calls, Asked::InTurn, Instant::now())?;
        // The read marks one under way as it begins, so what one that doesn't finish waits goes by the read before.
        let before = self.pull_requests.schedules.lock().get(&id).copied();
        let (hub, read_id) = (self.clone(), id.clone());
        let read = async move { hub.read_in_turn(&read_id, path, calls).await };
        let hub = self.clone();
        let after = move |ended: Ended, _: Limits| async move {
            // So one that didn't finish doesn't come up again soon, ahead of the rest.
            if !ended.finished_or_ran_uncut() {
                hub.pull_requests.wait_longer_than(&workspace.id, before);
            }
        };
        self.start_read(JobKind::ReadPullRequestInTurn, id, IN_TURN_RUNS, read, after).await;
        Ok(())
    }

    /// Reads one nobody asked for, through its worker, starting one if it has none.
    async fn read_in_turn(self: &Arc<Self>, id: &WorkspaceId, path: String, calls: u32) {
        let link = match self.worker_to_read(id).await {
            Ok(link) => link,
            Err(e) => {
                debug!(%id, "starting a worker to read the pull request: {}", e.message);
                self.pull_requests.wait_longer(id);
                return;
            }
        };
        // One the budget can't spare once GitHub has said what's left is left for the next look, which finds the same.
        if self.spend(&link, &path, calls, Asked::InTurn).await.is_err() {
            return;
        }
        if let Err(e) =
            self.read_pull_request(id, path, &link, Asked::InTurn, When::Always).await.and_then(Answer::or_known)
        {
            debug!(%id, "reading the pull request in turn: {}", e.message);
        }
    }

    /// Starts `read` as a job of `kind` about the workspace, within the limits of a read that makes `runs` runs of
    /// `gh`, as [`Hub::read_limits`] says. Its after-step runs `after` with how it ended, then lets go of the
    /// workspace's worker, as after every read, so one that started a worker leaves none running however it ended.
    pub(in crate::hub::pull_requests) async fn start_read<R, A>(
        self: &Arc<Self>,
        kind: JobKind,
        id: WorkspaceId,
        runs: u32,
        read: R,
        after: impl FnOnce(Ended, Limits) -> A + Send + 'static,
    ) where
        R: Future<Output = ()> + Send + 'static,
        A: Future<Output = ()> + Send + 'static,
    {
        let limits = self.read_limits(&id, runs).await;
        let (hub, key) = (self.clone(), id.clone());
        let afterwards = move |ended: Ended| async move {
            after(ended, limits).await;
            hub.let_go_after_read(&key).await;
        };
        if let Err(not) = self.jobs.start(kind, id, limits, read, afterwards) {
            debug!(?not, "a read didn't start: {kind}");
        }
    }

    /// How long a read of the workspace's pull requests that makes `runs` runs of `gh` has: [`BESIDES_GH`], each run
    /// taking all the [`GH_RUNS_WITHIN`] the worker gives it, and if the workspace has no worker running, as long as
    /// its provider takes to clear the instance and start one, and the worker has to answer the hub's hello. Then, to
    /// stop the worker again and clear its instance, the worker's [`STOP_GRACE`] and its provider's time again.
    pub(super) async fn read_limits(&self, id: &WorkspaceId, runs: u32) -> Limits {
        // One whose workspace or provider can't be found fails at once, so it needs no more.
        let provider = self.workspace(id).await.and_then(|workspace| self.provider(&workspace.provider));
        let provider_takes = provider.map_or(Duration::ZERO, |provider| provider.starts_or_stops_within());
        let starting = if self.has_worker(id) {
            Duration::ZERO
        } else {
            provider_takes.saturating_mul(2).saturating_add(self.timers.hello_within)
        };
        let work = BESIDES_GH.saturating_add(GH_RUNS_WITHIN.saturating_mul(runs)).saturating_add(starting);
        Limits { work, after: STOP_GRACE.saturating_add(provider_takes) }
    }
}
