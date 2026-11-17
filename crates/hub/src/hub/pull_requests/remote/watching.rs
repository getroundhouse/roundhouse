use super::{
    ApiError, Duration, ErrorCode, Group, GroupKey, HashMap, Hub, Instant, PullRequestNumber, PullRequestsSnapshot,
    WorkspaceId,
};
use crate::jobs::{Ended, Limits};

/// Active waits get the same two-minute cadence as the existing PR watcher. Transient failures back off to 30 min.
const EVERY: Duration = Duration::from_secs(120);
pub(super) const BACKOFF_MAX: Duration = Duration::from_mins(30);

pub(in crate::hub::pull_requests) struct RemoteSchedule {
    pub(in crate::hub::pull_requests) next: Instant,
    pub(in crate::hub::pull_requests) failures: u32,
}

impl Hub {
    /// Schedules the group's next read by how this one went: [`EVERY`] after one that worked, and backing off to
    /// [`BACKOFF_MAX`] after each that failed in a row.
    pub(super) fn remote_read_went(&self, key: GroupKey, went: Result<(), ErrorCode>) {
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

/// The groups due to be read at `now`, the one due longest first.
pub(super) fn due(
    groups: std::collections::BTreeMap<GroupKey, Group>,
    schedules: &HashMap<GroupKey, RemoteSchedule>,
    now: Instant,
) -> Vec<(GroupKey, Group)> {
    let mut due: Vec<_> =
        groups.into_iter().filter(|(key, _)| schedules.get(key).is_none_or(|s| s.next <= now)).collect();
    due.sort_by_key(|(key, _)| schedules.get(key).map_or(now, |s| s.next));
    due
}

/// What a read that didn't finish is recorded as, like a read that failed, or `None` for one that did, or that got as far
/// as recording what it read. Its waits' agents read it, so it says what happened in their terms, and the log says
/// where the read had got to.
pub(super) fn unfinished(ended: &Ended, limits: Limits) -> Option<ApiError> {
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
