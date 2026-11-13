use super::super::{Hub, new_id, now_ms};
use super::rules;
use crate::worker_link::WorkerLink;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    PullRequestFound, Session, SessionId, ToolUseId, Wait, WaitEnd, WaitEnded, WaitId, WaitUntil, WorkspaceId,
};
use protocol::worker::WaitAnswer;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Well within the minute a worker waits for its hub to answer an ask, so a wait is never recorded after the agent that
/// asked for it has stopped waiting to hear of it, which would leave it for nobody.
const START_WITHIN: Duration = Duration::from_secs(40);

impl Hub {
    /// Starts a wait for the agent in `session_id`, of `workspace_id`'s, on its pull request as it's read now, and
    /// ends the session's open one, since an agent waits on one at a time. One whose pull request is already done, or
    /// needs the agent, ends at once, and leaves the open one going. A branch without one is refused, and so is a worktree with commits not pushed,
    /// and a read that fails, or doesn't finish within [`START_WITHIN`], rather than start from what was known before
    /// the agent pushed. A call that started one already, as when its CLI asks again, gets that one.
    pub(crate) async fn start_wait(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        link: &WorkerLink,
        session_id: SessionId,
        until: WaitUntil,
        call: Option<ToolUseId>,
    ) -> Result<WaitAnswer, ApiError> {
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
            watch: None,
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
        self.announce_chat_of(&wait).await;
        Ok(WaitAnswer { wait, pull_request: Some(pull_request) })
    }

    /// The wait `call` started in the session, if it started one.
    pub(super) async fn started_by(
        &self,
        session_id: &SessionId,
        call: Option<&ToolUseId>,
    ) -> Result<Option<Wait>, ApiError> {
        let Some(call) = call.cloned() else { return Ok(None) };
        let session_id = session_id.clone();
        self.blocking(move |store| store.wait_of_call(&session_id, &call)).await
    }

    /// One of the session's waits, and the pull request as it was last read, which costs no call to GitHub.
    pub(crate) async fn wait_of(&self, session_id: &SessionId, id: WaitId) -> Result<WaitAnswer, ApiError> {
        let wait = self.own_wait(session_id, id).await?;
        self.with_pull_request(wait).await
    }

    /// Ends one of the session's waits as cancelled, unless it has ended already, and says how it stands.
    pub(crate) async fn cancel_wait(&self, session_id: &SessionId, id: WaitId) -> Result<WaitAnswer, ApiError> {
        let mut wait = self.own_wait(session_id, id).await?;
        if wait.ended.is_none() {
            let (id, ended) = (wait.id.clone(), WaitEnded { at_ms: now_ms(), how: WaitEnd::Cancelled });
            match self.blocking(move |store| store.end_wait(&id, ended)).await? {
                Some(cancelled) => {
                    info!(wait = %cancelled.id, "an agent cancelled its wait");
                    self.announce_chat_of(&cancelled).await;
                    wait = cancelled;
                }
                // A read ended it meanwhile.
                None => wait = self.own_wait(session_id, wait.id).await?,
            }
        }
        self.with_pull_request(wait).await
    }

    /// Calls off the chat's wait `wait`, for the person, as [`crate::session::LiveSession::call_off_wait`] says, and
    /// answers with the chat.
    pub(crate) async fn session_call_off_wait(
        self: &Arc<Self>,
        id: SessionId,
        wait: WaitId,
    ) -> Result<Session, ApiError> {
        self.live_session(&id).await?.call_off_wait(wait).await
    }

    /// Calls off the workspace's waits as it starts being archived, so none ending meanwhile starts an agent in a
    /// worktree that's about to go.
    pub(crate) async fn call_off_waits_of(&self, workspace_id: &WorkspaceId) -> Result<(), ApiError> {
        let (id, at_ms) = (workspace_id.clone(), now_ms());
        for wait in self.blocking(move |store| store.call_off_workspace_waits(&id, at_ms)).await? {
            info!(wait = %wait.id, workspace = %workspace_id, "archiving its workspace called off a wait");
            self.announce_chat_of(&wait).await;
        }
        Ok(())
    }

    pub(super) async fn own_wait(&self, session_id: &SessionId, id: WaitId) -> Result<Wait, ApiError> {
        let wait = self.blocking(move |store| store.wait(&id)).await?;
        if wait.session_id != *session_id {
            return Err(ApiError::new(ErrorCode::NotFound, format!("this chat has no wait {}", wait.id)));
        }
        Ok(wait)
    }

    /// Tells everyone following the feed what the wait's chat is now, read back from the store, once the wait has
    /// started or ended, which the chat's record shows. A failure is only logged, since the wait has started or ended.
    pub(super) async fn announce_chat_of(&self, wait: &Wait) {
        let id = wait.session_id.clone();
        match self.blocking(move |store| store.get_session(&id)).await {
            Ok(stored) => self.notify(HubNotice::Session(stored.session)),
            Err(e) => error!(session = %wait.session_id, "could not announce a change to a chat's wait: {e}"),
        }
    }

    pub(super) async fn with_pull_request(&self, wait: Wait) -> Result<WaitAnswer, ApiError> {
        let pull_request = self.workspace(&wait.workspace_id).await?.known_pull_request().cloned();
        Ok(WaitAnswer { wait, pull_request })
    }

    /// Has the session's agent started, if it isn't running, as after the hub started again, so its CLI asks after its
    /// task and hears how the wait ended. The worker that read the pull request is kept for it, rather than stopped as
    /// one started only for a read is, since the agent can only pick its task up again where its tools were.
    /// *Unconfirmed:* without asking.
    pub(super) async fn wake_for(self: &Arc<Self>, wait: &Wait) {
        let _kept = self.existing_worker(&wait.workspace_id);
        match self.live_session(&wait.session_id).await {
            Ok(session) => session.wake_agent(),
            Err(e) => warn!(session = %wait.session_id, "couldn't wake the agent whose wait ended: {}", e.message),
        }
    }

    /// Ends the workspace's waits that a read of its pull request ends, from what it found. Only for a read that
    /// worked, since one that failed says nothing of the pull request.
    pub(in super::super) async fn end_waits(self: &Arc<Self>, id: &WorkspaceId, found: &PullRequestFound) {
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
                    self.announce_chat_of(&wait).await;
                    self.wake_for(&wait).await;
                }
            }
            Err(e) => warn!(%id, "couldn't judge the workspace's waits after reading its pull request: {}", e.message),
        }
    }
}
