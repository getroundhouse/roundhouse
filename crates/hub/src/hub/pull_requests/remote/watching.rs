use tracing::warn;

use super::{
    ApiError, Arc, Asked, Duration, ErrorCode, GitHubRepository, Group, GroupKey, HashMap, Hub, Instant,
    PullRequestNumber, PullRequestSelection, PullRequestsSnapshot, ReadRemotePullRequests, ReadsHeld, WorkerLink,
    WorkspaceId, groups,
};
use crate::jobs::{self, Ended, JobKind, Limits};

/// Active waits get the same two-minute cadence as the existing PR watcher. Transient failures back off to 30 min.
const EVERY: Duration = Duration::from_secs(120);
pub(super) const BACKOFF_MAX: Duration = Duration::from_mins(30);

pub(in crate::hub::pull_requests) struct RemoteSchedule {
    pub(super) next: Instant,
    pub(super) failures: u32,
}

pub(in crate::hub) struct ReadFailed {
    pub(in crate::hub) error: ApiError,
    pub(in crate::hub) affected: Vec<PullRequestNumber>,
}

impl From<ApiError> for ReadFailed {
    fn from(error: ApiError) -> Self {
        Self { error, affected: Vec::new() }
    }
}

impl Hub {
    pub(in crate::hub) async fn read_remote_holding(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &WorkerLink,
        repository: Option<GitHubRepository>,
        selection: PullRequestSelection,
        agent: bool,
    ) -> Result<(PullRequestsSnapshot, ReadsHeld), ReadFailed> {
        jobs::step("waiting for the workspace's turn to read its pull requests");
        let gate = self.pull_requests.gates.lock().entry(id.clone()).or_default().clone();
        let held = gate.lock_owned().await;
        if agent {
            self.pull_requests.space_out(id, self.timers.agent_spacing).await;
        }
        self.pull_requests.remote_read_at.lock().insert(id.clone(), Instant::now());
        let path = self.ready_workspace(id).await?.path().to_owned();
        let asked = if agent { Asked::Agent } else { Asked::InTurn };
        self.spend(link, &path, 1, asked)
            .await
            .map_err(|_| ApiError::new(ErrorCode::Worker, "GitHub budget is temporarily exhausted"))?;
        jobs::step(format!("waiting for {link} to read the pull requests"));
        let reply = link.call(ReadRemotePullRequests { path, repository, pull_requests: selection }).await.map_err(
            |error| {
                // Transport loss is retryable; only the worker's completed read can report GitHub access lost.
                if error.code == ErrorCode::Unsupported {
                    error
                } else {
                    ApiError::new(ErrorCode::Worker, error.message)
                }
            },
        )?;
        let mut budget = self.pull_requests.budget.lock();
        budget.spent(reply.cost.saturating_sub(1), Instant::now());
        if let Some(limit) = reply.rate_limit {
            budget.github_said(limit, Instant::now(), super::super::now_ms());
        }
        let snapshot = reply.observation.map_err(|error| ReadFailed { error, affected: reply.affected })?;
        Ok((snapshot, held))
    }

    /// Starts reading the oldest due group of waits' pull requests, as a job, unless a read of its workspace is
    /// running. Overlapping waits share a read, and disjoint scopes keep their own backoff. A pinned repository is part
    /// of the key, so changed remotes can't redirect it. No task's `tasks/get` reaches GitHub.
    pub(in crate::hub::pull_requests) async fn start_due_wait_read(self: &Arc<Self>) {
        if !self.jobs.has_room(JobKind::ReadWaitedPullRequests) {
            return;
        }
        let waits = match self.blocking(crate::store::Store::remote_waits).await {
            Ok(waits) => waits,
            Err(e) => return warn!("reading which waits to read the pull requests of: {}", e.message),
        };
        let now = Instant::now();
        let groups = groups(waits);
        let busy = self.jobs.busy();
        let due = {
            let mut schedules = self.pull_requests.remote.lock();
            schedules.retain(|key, _| groups.contains_key(key));
            let free = groups.into_iter().filter(|(key, _)| !busy.contains(&key.0)).collect();
            oldest_due(free, &schedules, now)
        };
        let Some((key, Group { repository, selection })) = due else { return };
        let id = key.0.clone();
        let runs = group_runs(selection.numbers().len());
        let hub = self.clone();
        let (read_key, pinned) = (key.clone(), repository.clone());
        let read = async move { hub.read_group(read_key, pinned, selection).await };
        let hub = self.clone();
        let after = move |ended: Ended, limits: Limits| async move {
            hub.after_group_read(key, &repository, &ended, limits).await;
        };
        self.start_read(JobKind::ReadWaitedPullRequests, id, runs, read, after).await;
    }

    /// A group's read, through its workspace's worker, starting one if it has none. Once it has been read, what it
    /// found is recorded on the waits, and its next read scheduled, however the job goes on.
    async fn read_group(
        self: &Arc<Self>,
        key: GroupKey,
        repository: GitHubRepository,
        selection: PullRequestSelection,
    ) {
        let id = key.0.clone();
        let read = match self.worker_to_read(&id).await {
            Ok(link) => self.read_remote_holding(&id, &link, Some(repository.clone()), selection, false).await,
            Err(error) => Err(ApiError::new(ErrorCode::Worker, error.message).into()),
        };
        let hub = self.clone();
        jobs::uncut(async move {
            let went = match read {
                Ok((snapshot, _held)) => {
                    hub.observe_remote_waits(&id, &snapshot).await;
                    Ok(())
                }
                Err(failed) => {
                    hub.remote_waits_failed(&id, &repository, &failed.error, &failed.affected).await;
                    Err(failed.error.code)
                }
            };
            hub.remote_read_went(key, went);
        })
        .await;
    }

    /// What a group's read that didn't finish leaves: it backs off as one that failed does, and its waits record it as
    /// one, so their agents can tell they aren't being read.
    async fn after_group_read(
        self: &Arc<Self>,
        key: GroupKey,
        repository: &GitHubRepository,
        ended: &Ended,
        limits: Limits,
    ) {
        if let Some(error) = unfinished(ended, limits) {
            // First, since it waits on nothing, so the group backs off even if what follows doesn't finish in time.
            self.remote_read_went(key.clone(), Err(error.code));
            self.remote_waits_failed(&key.0, repository, &error, &[]).await;
        }
    }

    /// Schedules the group's next read by how this one went: [`EVERY`] after one that worked, and backing off to
    /// [`BACKOFF_MAX`] after each that failed in a row.
    fn remote_read_went(&self, key: GroupKey, went: Result<(), ErrorCode>) {
        let mut schedules = self.pull_requests.remote.lock();
        let previous = schedules.get(&key).map_or(0, |s| s.failures);
        // Revision churn is a successful contact with GitHub, not an outage worth backing off from.
        let superseded = went == Err(ErrorCode::Conflict);
        let failures = if went.is_ok() || superseded { 0 } else { previous.saturating_add(1).min(4) };
        let delay = EVERY.saturating_mul(2_u32.pow(failures)).min(BACKOFF_MAX);
        schedules.insert(key, RemoteSchedule { next: Instant::now() + delay, failures });
    }

    /// Registers a fresh read's cadence; polling must not immediately repeat a just-started wait's read.
    pub(in crate::hub) fn remote_was_read(&self, id: &WorkspaceId, snapshot: &PullRequestsSnapshot) {
        self.pull_requests
            .remote
            .lock()
            .entry((
                id.clone(),
                snapshot.repository.url.clone(),
                snapshot
                    .pull_requests
                    .iter()
                    .filter_map(|pr| PullRequestNumber::try_from(pr.pull_request.number).ok())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            ))
            .or_insert(RemoteSchedule { next: Instant::now() + EVERY, failures: 0 });
    }
}

pub(super) fn oldest_due(
    groups: std::collections::BTreeMap<GroupKey, Group>,
    schedules: &HashMap<GroupKey, RemoteSchedule>,
    now: Instant,
) -> Option<(GroupKey, Group)> {
    groups
        .into_iter()
        .filter(|(key, _)| schedules.get(key).is_none_or(|s| s.next <= now))
        .min_by_key(|(key, _)| schedules.get(key).map_or(now, |s| s.next))
}

/// The runs of `gh` a group's read makes for `watched` pull requests: asking how much of GitHub's limit is left, and for
/// each, its details twice, its checks and its base's rules, a page of each.
pub(super) fn group_runs(watched: usize) -> u32 {
    u32::try_from(watched).unwrap_or(u32::MAX).saturating_mul(4).saturating_add(1)
}

/// What a read that didn't finish is recorded as, like a read that failed, or `None` for one that did, or that got as far
/// as recording what it read. Its waits' agents read it, so it says what happened in their terms, and the log says
/// where the read had got to.
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
