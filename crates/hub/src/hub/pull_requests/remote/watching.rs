use super::{
    ApiError, Arc, Asked, Duration, ErrorCode, GitHubRepository, Group, GroupKey, HashMap, Hub, Instant,
    PullRequestNumber, PullRequestSelection, PullRequestsSnapshot, ReadRemotePullRequests, ReadsHeld, WorkerLink,
    WorkspaceId, groups,
};

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
        let gate = self.pull_requests.gates.lock().entry(id.clone()).or_default().clone();
        let held = gate.lock_owned().await;
        if agent {
            self.pull_requests.space_out(id).await;
        }
        self.pull_requests.remote_read_at.lock().insert(id.clone(), Instant::now());
        let path = self.ready_workspace(id).await?.path().to_owned();
        let asked = if agent { Asked::Agent } else { Asked::InTurn };
        self.spend(link, &path, 1, asked)
            .await
            .map_err(|_| ApiError::new(ErrorCode::Worker, "GitHub budget is temporarily exhausted"))?;
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

    /// One due dependency group per tick. Overlapping waits share a read; disjoint scopes keep their own backoff. No task's
    /// tasks/get request reaches GitHub. A pinned repository is part of the key, so changed remotes cannot redirect it.
    pub(in crate::hub::pull_requests) async fn read_remote_due(self: &Arc<Self>) {
        let Ok(waits) = self.blocking(crate::store::Store::remote_waits).await else { return };
        let now = Instant::now();
        let groups = groups(waits);
        let due = {
            let mut schedules = self.pull_requests.remote.lock();
            schedules.retain(|key, _| groups.contains_key(key));
            oldest_due(groups, &schedules, now)
        };
        let Some((key, Group { repository, selection })) = due else { return };
        let id = &key.0;
        let result = self.read_remote_in_turn(id, repository, selection).await;
        let mut schedules = self.pull_requests.remote.lock();
        let previous = schedules.get(&key).map_or(0, |s| s.failures);
        // Revision churn is a successful contact with GitHub, not an outage worth backing off from.
        let superseded = result.as_ref().is_err_and(|error| error.code == ErrorCode::Conflict);
        let failures = if result.is_ok() || superseded { 0 } else { previous.saturating_add(1).min(4) };
        let delay = EVERY.saturating_mul(2_u32.pow(failures)).min(BACKOFF_MAX);
        schedules.insert(key, RemoteSchedule { next: Instant::now() + delay, failures });
    }

    async fn read_remote_in_turn(
        self: &Arc<Self>,
        id: &WorkspaceId,
        repository: GitHubRepository,
        selection: PullRequestSelection,
    ) -> Result<(), ApiError> {
        let workspace = self.workspace(id).await?;
        let link = match self.worker_to_read(id).await {
            Ok(link) => link,
            Err(error) => {
                let error = ApiError::new(ErrorCode::Worker, error.message);
                self.remote_waits_failed(id, &repository, &error, &[]).await;
                return Err(error);
            }
        };
        let read = self.read_remote_holding(id, &link, Some(repository.clone()), selection, false).await;
        let outcome = match read {
            Ok((snapshot, _held)) => {
                self.observe_remote_waits(id, &snapshot).await;
                Ok(())
            }
            Err(error) => {
                self.remote_waits_failed(id, &repository, &error.error, &error.affected).await;
                Err(error.error)
            }
        };
        self.stop_worker_only_read(&workspace, &link).await;
        outcome
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
