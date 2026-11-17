//! The pull requests an agent asks for, or starts a wait on, read by the hub itself from GitHub, so a read needs no
//! worker to call GitHub, and every agent reads as the default does.

use super::WorkspaceId;
use super::{ApiError, Arc, Asked, ErrorCode, Hub, Instant, PullRequestSelection, PullRequestsSnapshot, ReadsHeld};
use crate::github::{PullRequestsAsk, PullRequestsRead};
use crate::jobs;

impl Hub {
    /// Reads `selection` of the workspace's repository on GitHub, and those open under them in their stacks, for an
    /// agent, one of its agents' reads at a time, and no sooner than [`crate::Timers::agent_spacing`] after the last.
    /// Returns its agents' reads held back, for a wait to be recorded before another of them can judge it.
    pub(in crate::hub) async fn read_remote_for_agent(
        self: &Arc<Self>,
        id: &WorkspaceId,
        selection: PullRequestSelection,
    ) -> Result<(PullRequestsSnapshot, ReadsHeld), ApiError> {
        jobs::step("waiting for the workspace's turn to read its pull requests");
        let gate = self.pull_requests.agent_gates.lock().entry(id.clone()).or_default().clone();
        let held = gate.lock_owned().await;
        self.pull_requests.space_out(id, self.timers.agent_spacing).await;
        self.pull_requests.remote_read_at.lock().insert(id.clone(), Instant::now());
        let workspace = self.workspace(id).await?;
        let repository = self.on_github(&workspace).await?;
        let (reads_as, token) = self.reading_as(&workspace).await?;
        self.pull_requests.budget.lock().take(1, Asked::Agent, Instant::now()).map_err(|_| {
            ApiError::new(ErrorCode::Worker, "GitHub's limit for reading pull requests is used up until it resets")
        })?;
        jobs::step("waiting for GitHub to answer for the pull requests");
        let read = self.github.read_pull_requests(&token, vec![PullRequestsAsk { repository, selection }]).await;
        self.spent_reading(&read, 1);
        if read.refused {
            self.reading_token_refused(&reads_as);
        }
        let answer = read.answers.into_iter().next().ok_or_else(|| ApiError::new(ErrorCode::Worker, "no answer"))?;
        let snapshot = answer.map_err(|unread| unread.error)?;
        Ok((snapshot, held))
    }

    /// Counts what `read` spent against GitHub's limit, beyond the `taken` taken before it, with what GitHub said is
    /// left.
    pub(super) fn spent_reading(&self, read: &PullRequestsRead, taken: u32) {
        let mut budget = self.pull_requests.budget.lock();
        budget.spent(read.cost.saturating_sub(taken), Instant::now());
        if let Some(limit) = read.rate_limit {
            budget.github_said(limit, Instant::now(), crate::store::now_ms());
        }
    }
}
