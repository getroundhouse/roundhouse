use super::super::{Hub, now_ms};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Session, SessionId, ToolUseId, Wait, WaitEnd, WaitEnded, WaitId, WorkspaceId};
use protocol::worker::WaitAnswer;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Well within the minute a worker waits for its hub to answer an ask, so a wait is never recorded after the agent that
/// asked for it has stopped waiting to hear of it, which would leave it for nobody.
pub(super) const START_WITHIN: Duration = Duration::from_secs(40);

impl Hub {
    /// The wait `call` started in the session, if it started one.
    pub(crate) async fn started_by(
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
                    self.wait_reads_settled().await;
                    self.announce_chat_of(&cancelled).await;
                    wait = self.own_wait(session_id, cancelled.id).await?;
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
        let called_off = self.blocking(move |store| store.call_off_workspace_waits(&id, at_ms)).await?;
        self.wait_reads_settled().await;
        for wait in called_off {
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

    /// `wait`, with the workspace's own pull request where it's a wait on that. A wait on pull requests it watches
    /// carries what it read of them in its watch, apart from any worktree, so it has none.
    pub(super) async fn with_pull_request(&self, wait: Wait) -> Result<WaitAnswer, ApiError> {
        let wait = self.as_read(wait);
        let pull_request = match &wait.watch {
            Some(_) => None,
            None => self.known_of(&self.workspace(&wait.workspace_id).await?).and_then(|found| found.pull_request),
        };
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
}
