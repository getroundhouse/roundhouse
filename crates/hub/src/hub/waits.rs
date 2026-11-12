//! Agents' waits on their workspace's pull request: starting one from where the pull request stands, reading one, which
//! never reads GitHub, and ending them as each read of the pull request says. The hub keeps them, so they last across
//! its restarts, and the agent's MCP task is only a copy of one.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    PullRequest, PullRequestFound, SessionId, ToolUseId, Wait, WaitEnd, WaitEnded, WaitId, WaitUntil, WorkspaceId,
};
use tracing::{info, warn};

use super::{Hub, new_id, now_ms};
use crate::worker_link::WorkerLink;

mod rules;
#[cfg(test)]
mod tests;

/// Well within the minute a worker waits for its hub to answer an ask, so a wait is never recorded after the agent that
/// asked for it has stopped waiting to hear of it, which would leave it for nobody.
const START_WITHIN: Duration = Duration::from_secs(40);

impl Hub {
    /// Starts a wait for the agent in `session_id`, of `workspace_id`'s, on its pull request as it's read now, and
    /// ends the session's open one, since an agent waits on one at a time. One whose pull request is already done, or
    /// needs the agent, ends at once. A branch without one is refused, and so is a worktree with commits not pushed,
    /// and a read that fails, or doesn't finish within [`START_WITHIN`], rather than start from what was known before
    /// the agent pushed. A call that started one already, as when its CLI asks again, gets that one.
    pub(crate) async fn start_wait(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        link: &WorkerLink,
        session_id: SessionId,
        until: WaitUntil,
        call: Option<ToolUseId>,
    ) -> Result<(Wait, Option<PullRequest>), ApiError> {
        if let Some(started) = self.started_by(&session_id, call.as_ref()).await? {
            return self.with_pull_request(started).await;
        }
        let reading = tokio::time::timeout(START_WITHIN, self.pull_request_to_wait_on(workspace_id, link));
        let (pull_request, reads_held) = reading.await.map_err(|_| {
            ApiError::new(ErrorCode::Unavailable, format!("the pull request couldn't be read within {START_WITHIN:?}"))
        })??;
        let Some(pull_request) = pull_request else {
            return Err(ApiError::new(ErrorCode::NotFound, "this workspace's branch has no pull request to wait on"));
        };
        let started_at_ms = now_ms();
        let mut wait = Wait {
            id: new_id().into(),
            session_id,
            workspace_id: workspace_id.clone(),
            number: pull_request.number,
            call,
            until,
            started_at_ms,
            ended: None,
        };
        let seen = rules::seen_in(&pull_request);
        wait.ended = rules::ended(&wait, seen, Some(&pull_request)).map(|how| WaitEnded { at_ms: started_at_ms, how });
        if wait.ended.is_none()
            && let Some(why) = rules::not_yet(&pull_request)
        {
            return Err(ApiError::new(ErrorCode::Conflict, why));
        }
        let stored = wait.clone();
        let replaced = self.blocking(move |store| store.start_wait(&stored, seen)).await?;
        // Only once it's recorded, so the next read judges it.
        drop(reads_held);
        if let Some(replaced) = replaced {
            info!(wait = %replaced, "the agent's next wait replaced its last");
        }
        info!(wait = %wait.id, session = %wait.session_id, ended = ?wait.ended, "an agent started a wait");
        Ok((wait, Some(pull_request)))
    }

    /// The wait `call` started in the session, if it started one.
    async fn started_by(&self, session_id: &SessionId, call: Option<&ToolUseId>) -> Result<Option<Wait>, ApiError> {
        let Some(call) = call.cloned() else { return Ok(None) };
        let session_id = session_id.clone();
        self.blocking(move |store| store.wait_of_call(&session_id, &call)).await
    }

    /// One of the session's waits, and the pull request as it was last read, which costs no call to GitHub.
    pub(crate) async fn wait_of(
        &self,
        session_id: &SessionId,
        id: WaitId,
    ) -> Result<(Wait, Option<PullRequest>), ApiError> {
        let wait = self.own_wait(session_id, id).await?;
        self.with_pull_request(wait).await
    }

    /// Ends one of the session's waits as cancelled, unless it has ended already, and says how it stands.
    pub(crate) async fn cancel_wait(
        &self,
        session_id: &SessionId,
        id: WaitId,
    ) -> Result<(Wait, Option<PullRequest>), ApiError> {
        let mut wait = self.own_wait(session_id, id).await?;
        if wait.ended.is_none() {
            let (id, ended) = (wait.id.clone(), WaitEnded { at_ms: now_ms(), how: WaitEnd::Cancelled });
            match self.blocking(move |store| store.end_wait(&id, ended)).await? {
                Some(cancelled) => {
                    info!(wait = %cancelled.id, "an agent cancelled its wait");
                    wait = cancelled;
                }
                // A read ended it meanwhile.
                None => wait = self.own_wait(session_id, wait.id).await?,
            }
        }
        self.with_pull_request(wait).await
    }

    async fn own_wait(&self, session_id: &SessionId, id: WaitId) -> Result<Wait, ApiError> {
        let wait = self.blocking(move |store| store.wait(&id)).await?;
        if wait.session_id != *session_id {
            return Err(ApiError::new(ErrorCode::NotFound, format!("this chat has no wait {}", wait.id)));
        }
        Ok(wait)
    }

    async fn with_pull_request(&self, wait: Wait) -> Result<(Wait, Option<PullRequest>), ApiError> {
        let pull_request = self.workspace(&wait.workspace_id).await?.known_pull_request().cloned();
        Ok((wait, pull_request))
    }

    /// Ends the workspace's waits that a read of its pull request ends, from what it found. Only for a read that
    /// worked, since one that failed says nothing of the pull request.
    pub(super) async fn end_waits(&self, id: &WorkspaceId, found: &PullRequestFound) {
        let (owned, now, at_ms) = (id.clone(), found.pull_request.clone(), now_ms());
        let judged = self
            .blocking(move |store| {
                store.judge_waits(&owned, at_ms, |wait, seen| rules::ended(wait, seen, now.as_ref()))
            })
            .await;
        match judged {
            Ok(ended) => {
                for wait in ended {
                    info!(wait = %wait.id, how = ?wait.ended.map(|ended| ended.how), "a wait ended");
                }
            }
            Err(e) => warn!(%id, "couldn't judge the workspace's waits after reading its pull request: {}", e.message),
        }
    }
}
