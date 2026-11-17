//! Reading the pull requests the waits watch, as the hub's loop starts it: every group of waits that's due, read
//! together with the rest read as the same token, in one read, so many workspaces' waits cost a few queries a round
//! rather than a read each. Each token's read is a job of its own, so a GitHub that doesn't answer one holds up no
//! other's, and none needs a worker, but to fetch a machine's own `gh`'s token after the hub starts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use protocol::model::{GitHubRepository, GithubRepositoryName, GithubToken, ProviderName};
use tracing::{debug, warn};

use super::reading_as::ReadsAs;
use super::watching::{due, unfinished};
use super::{
    ApiError, Arc, Asked, ErrorCode, Group, GroupKey, Hub, Instant, PullRequestsSnapshot, WorkspaceId, groups,
};
use crate::github::{ASK_WITHIN, PullRequestsAsk, Unread, first_queries};
use crate::hub::found_github_logins::FOUND_WITHIN;
use crate::jobs::{self, Ended, JobKind, Limits};
use crate::store::now_ms;
use crate::worker_link::STOP_GRACE;

/// The queries a read of waits makes at most, for each its first round makes, each given the time the hub gives a call
/// to GitHub: two rounds for most, and as many again for pages and for what's under them in their stacks, read again
/// twice for pull requests that moved as they were read.
const QUERIES_AT_MOST: u32 = 12;

impl Hub {
    /// Starts reading the pull requests of every group of waits that's due, a job for each token they're read as, but
    /// for a token whose read is under way. A group whose workspace's default blocks fails as it would on GitHub. No
    /// task's `tasks/get` reaches GitHub.
    pub(in crate::hub::pull_requests) async fn start_due_wait_reads(self: &Arc<Self>) {
        let waits = match self.blocking(crate::store::Store::remote_waits).await {
            Ok(waits) => waits,
            Err(e) => return warn!("reading which waits to read the pull requests of: {}", e.message),
        };
        let groups = groups(waits);
        let due = {
            let mut schedules = self.pull_requests.remote.lock();
            schedules.retain(|key, _| groups.contains_key(key));
            due(groups, &schedules, Instant::now())
        };
        let mut by_token: Vec<(ReadsAs, Vec<(GroupKey, Group)>)> = Vec::new();
        for (key, group) in due {
            let reads_as = match self.workspace(&key.0).await.and_then(|workspace| self.reads_as(&workspace)) {
                Ok(reads_as) => reads_as,
                Err(error) => {
                    self.wait_read_went(key, &group, Err(Unread { error, affected: Vec::new() })).await;
                    continue;
                }
            };
            match by_token.iter_mut().find(|(read_as, _)| *read_as == reads_as) {
                Some((_, groups)) => groups.push((key, group)),
                None => by_token.push((reads_as, vec![(key, group)])),
            }
        }
        let busy = self.wait_reads.busy();
        for (reads_as, groups) in by_token.into_iter().filter(|(reads_as, _)| !busy.contains(reads_as)) {
            if !self.wait_reads.has_room(JobKind::ReadWaitedPullRequests) {
                return;
            }
            self.start_wait_read(reads_as, groups).await;
        }
    }

    /// Starts reading `groups` as `reads_as`, as a job, within the limits of a read that makes [`QUERIES_AT_MOST`]
    /// for each its first round makes, and may start the first group's workspace's worker for its machines' `gh`'s
    /// token. Once it ends, however it ended, one that didn't finish is recorded on each group's waits as a read that
    /// failed, and a worker it took is let go of, unless another read holds it.
    async fn start_wait_read(self: &Arc<Self>, reads_as: ReadsAs, groups: Vec<(GroupKey, Group)>) {
        let Some(id) = groups.first().map(|(key, _)| key.0.clone()) else { return };
        let queries = first_queries(&asks(&groups).0);
        let limits = self.wait_read_limits(&id, queries.saturating_mul(QUERIES_AT_MOST)).await;
        let took_worker = Arc::new(AtomicBool::new(false));
        let (hub, token_of, read, took) = (self.clone(), reads_as.clone(), groups.clone(), took_worker.clone());
        let work = async move { hub.read_waits_as(&token_of, read, &took).await };
        let hub = self.clone();
        let after = move |ended: Ended| async move {
            if let Some(error) = unfinished(&ended, limits) {
                for (key, group) in groups {
                    let unread = Unread { error: error.clone(), affected: Vec::new() };
                    hub.wait_read_went(key, &group, Err(unread)).await;
                }
            }
            if took_worker.load(Ordering::Relaxed) {
                hub.let_go_after_read(&id).await;
            }
        };
        if let Err(not) = self.wait_reads.start(JobKind::ReadWaitedPullRequests, reads_as, limits, work, after) {
            debug!(?not, "a read of waits didn't start");
        }
    }

    /// How long a read of waits that makes `queries` has: the time the hub gives a call to GitHub for each, and for a
    /// machine's own token, the time to start the workspace's worker and for it to hand that over. Then as long as
    /// a worker that was started takes to stop.
    pub(in crate::hub::pull_requests) async fn wait_read_limits(&self, id: &WorkspaceId, queries: u32) -> Limits {
        let provider = self.workspace(id).await.and_then(|workspace| self.provider(&workspace.provider));
        let provider_takes = provider.map_or(Duration::ZERO, |provider| provider.starts_or_stops_within());
        let starting = provider_takes.saturating_mul(2).saturating_add(self.timers.hello_within);
        let work = ASK_WITHIN.saturating_mul(queries).saturating_add(starting).saturating_add(FOUND_WITHIN);
        Limits { work, after: STOP_GRACE.saturating_add(provider_takes) }
    }

    /// Reads `groups` as `reads_as`, together, and records what each found on its waits, however the job goes on
    /// once it has. One that GitHub's count for the token can't spare now stays due, for the next look. A read whose token can't be had,
    /// or which GitHub refuses, is recorded on the waits as one that may work later, so they go on waiting. `took`
    /// says whether it took a worker, for the token.
    async fn read_waits_as(self: &Arc<Self>, reads_as: &ReadsAs, groups: Vec<(GroupKey, Group)>, took: &AtomicBool) {
        let token = match (reads_as, groups.first()) {
            (ReadsAs::MachineOwn(provider), Some((key, _))) => {
                self.machine_token_for_waits(provider, &key.0, took).await
            }
            _ => self.token_of(reads_as, Duration::ZERO).await,
        };
        let token = match token {
            Ok(token) => token,
            Err(error) => {
                for (key, group) in groups {
                    self.wait_read_went(key, &group, Err(Unread { error: error.clone(), affected: Vec::new() })).await;
                }
                return;
            }
        };
        if self.pull_requests.limits.allows(reads_as, Asked::InTurn, now_ms()).is_err() {
            return;
        }
        let (asks, asked, unasked) = asks(&groups);
        jobs::step(format!("waiting for GitHub to answer for {} groups of waits", asks.len()));
        let read = self.github.read_pull_requests(&token, asks, Asked::InTurn.leaving()).await;
        self.pull_requests.limits.said(reads_as, read.rate_limit);
        if read.refused {
            self.reading_token_refused(reads_as);
        }
        let hub = self.clone();
        jobs::uncut(async move {
            for ((key, group), answer) in asked.into_iter().zip(read.answers) {
                hub.wait_read_went(key, &group, answer).await;
            }
            for (key, group, error) in unasked {
                hub.wait_read_went(key, &group, Err(Unread { error, affected: Vec::new() })).await;
            }
        })
        .await;
    }

    /// The token of `provider`'s machines' own `gh`, for waits in the workspace `id`: the one handed the hub, or else
    /// from the workspace's worker, started for it if it has none, and held until it has, as [`Hub::machine_token`]
    /// asks for it. `took` is set once a worker is held, so the read lets go of it once it's done.
    pub(in crate::hub::pull_requests) async fn machine_token_for_waits(
        self: &Arc<Self>,
        provider: &ProviderName,
        id: &WorkspaceId,
        took: &AtomicBool,
    ) -> Result<GithubToken, ApiError> {
        if let Ok(token) = self.token_of(&ReadsAs::MachineOwn(provider.clone()), Duration::ZERO).await {
            return Ok(token);
        }
        let held = self.worker_to_read(id).await?;
        took.store(true, Ordering::Relaxed);
        jobs::step("waiting for the worker to hand over its machine's gh token");
        self.machine_token(provider, Some(held.link())).await
    }

    /// Records what a group's read found on its waits, or that it failed, and schedules its next read by that.
    async fn wait_read_went(
        self: &Arc<Self>,
        key: GroupKey,
        group: &Group,
        answer: Result<PullRequestsSnapshot, Unread>,
    ) {
        let went = match answer {
            Ok(snapshot) => {
                self.observe_remote_waits(&key.0, &snapshot).await;
                Ok(())
            }
            Err(Unread { error, affected }) => {
                self.remote_waits_failed(&key.0, &group.repository, &error, &affected).await;
                Err(error.code)
            }
        };
        self.remote_read_went(key, went);
    }
}

/// What reading `groups` asks of GitHub, an ask for each group on github.com, with those groups, in the same order, and
/// those not on it, with why they can't be read.
#[expect(clippy::type_complexity, reason = "the three are taken apart as soon as they're made")]
fn asks(
    groups: &[(GroupKey, Group)],
) -> (Vec<PullRequestsAsk>, Vec<(GroupKey, Group)>, Vec<(GroupKey, Group, ApiError)>) {
    let (mut asks, mut asked, mut unasked) = (Vec::new(), Vec::new(), Vec::new());
    for (key, group) in groups.iter().cloned() {
        match on_github(&group.repository) {
            Ok(repository) => {
                asks.push(PullRequestsAsk { repository, selection: group.selection.clone() });
                asked.push((key, group));
            }
            Err(error) => unasked.push((key, group, error)),
        }
    }
    (asks, asked, unasked)
}

/// Which repository on github.com a wait's pinned repository is, or unsupported for one elsewhere, as a wait started
/// through a worker on another GitHub could be, which the hub doesn't read.
fn on_github(repository: &GitHubRepository) -> Result<GithubRepositoryName, ApiError> {
    let on_github = repository.url.starts_with("https://github.com/");
    GithubRepositoryName::try_from(repository.name_with_owner.clone())
        .ok()
        .filter(|_| on_github)
        .ok_or_else(|| ApiError::new(ErrorCode::Unsupported, "the hub reads pull requests on github.com alone"))
}
