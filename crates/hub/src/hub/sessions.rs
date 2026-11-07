//! The session requests, most of which pass straight to the live session.

use std::sync::Arc;

use protocol::client::{ApiError, ClientResponse, ErrorCode, HubNotice};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::model::{AgentKind, Seq, Session, SessionStatus, WorkspaceStatus};
use tokio::sync::oneshot;

use super::{Hub, new_id, not_ready};
use crate::error::internal;
use crate::session::Command;
use crate::store::{StoredSession, now_ms};

impl Hub {
    pub(crate) async fn session_create(
        self: &Arc<Self>,
        workspace_id: String,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session, ApiError> {
        if agent == AgentKind::Unknown {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't know that kind of agent"));
        }
        let workspace = self.workspace(&workspace_id).await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let id = new_id();
        let now = now_ms();
        let stored = StoredSession {
            session: Session {
                id: id.clone(),
                workspace_id,
                agent,
                model,
                permission_mode,
                status: SessionStatus::Idle,
                last_seq: 0,
                created_at_ms: now,
                updated_at_ms: now,
                title: None,
                closed_at_ms: None,
            },
            // The hub's ID doubles as the agent's, so `--resume` works after a crash even if the
            // agent never reported one.
            agent_session_id: id,
            agent_opened: false,
        };
        let session = stored.session.clone();
        self.blocking(move |store| store.insert_session(&stored)).await?;
        self.notify(HubNotice::Session(session.clone()));
        Ok(session)
    }

    pub(crate) async fn session_ensure(
        self: &Arc<Self>,
        workspace_id: String,
        agent: AgentKind,
    ) -> Result<Session, ApiError> {
        let _one_at_a_time = self.ensuring_session.lock().await;
        let sessions = self.session_list(workspace_id.clone()).await?;
        match sessions.into_iter().rfind(|s| s.closed_at_ms.is_none()) {
            Some(open) => Ok(open),
            None => self.session_create(workspace_id, agent, None, None).await,
        }
    }

    pub(crate) async fn session_list(&self, workspace_id: String) -> Result<Vec<Session>, ApiError> {
        self.blocking(move |store| store.list_sessions(&workspace_id)).await
    }

    pub(crate) async fn session_get(&self, id: String) -> Result<Session, ApiError> {
        self.blocking(move |store| store.get_session(&id).map(|stored| stored.session)).await
    }

    pub(crate) async fn session_read(
        &self,
        id: String,
        from_seq: Seq,
        limit: u32,
        max_bytes: Option<u64>,
        include_raw: bool,
    ) -> Result<ClientResponse, ApiError> {
        let max_bytes = max_bytes.unwrap_or(crate::server::DEFAULT_READ_BYTES);
        let limit = limit.clamp(1, crate::server::MAX_READ_EVENTS);
        self.blocking(move |store| {
            let last_seq = store.get_session(&id)?.session.last_seq;
            let events = store.read_events(&id, from_seq, limit, max_bytes, include_raw)?;
            Ok(ClientResponse::Events { events, last_seq })
        })
        .await
    }

    /// Reads for a subscriber, never past `through`.
    pub(crate) async fn read_page(
        &self,
        id: String,
        from_seq: Seq,
        through: Seq,
    ) -> Result<Vec<SessionEvent>, ApiError> {
        let limit = u32::try_from(through - from_seq + 1)
            .map_or(crate::server::PUMP_PAGE_EVENTS, |n| n.min(crate::server::PUMP_PAGE_EVENTS));
        self.blocking(move |store| store.read_events(&id, from_seq, limit, crate::server::DEFAULT_READ_BYTES, false))
            .await
    }

    pub(crate) async fn session_prompt(
        self: &Arc<Self>,
        id: String,
        text: String,
        client_msg_id: Option<String>,
    ) -> Result<(Seq, bool), ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a prompt can't be empty"));
        }
        let session = self.live_session(&id).await?;
        let (reply, recorded) = oneshot::channel();
        session.command(Command::Prompt { text, client_msg_id, reply }).await?;
        recorded.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_approve(
        self: &Arc<Self>,
        id: String,
        approval_id: String,
        decision: ApprovalDecision,
    ) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        let (reply, done) = oneshot::channel();
        session.command(Command::Approve { approval_id, decision, reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_configure(
        self: &Arc<Self>,
        id: String,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session, ApiError> {
        // Not through the session's queue, because a prompt waiting on setup holds that. The choice
        // has to apply when that prompt finally starts the agent.
        self.live_session(&id).await?.configure(model, permission_mode).await
    }

    pub(crate) async fn session_close(self: &Arc<Self>, id: String) -> Result<Session, ApiError> {
        self.live_session(&id).await?.close().await
    }

    pub(crate) async fn session_reopen(self: &Arc<Self>, id: String) -> Result<Session, ApiError> {
        self.live_session(&id).await?.reopen().await
    }

    pub(crate) async fn session_cancel(self: &Arc<Self>, id: String) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        // A prompt waiting on setup holds this session's commands, so stopping has to reach setup directly.
        self.cancel_setup(&session.workspace_id).await;
        let (reply, done) = oneshot::channel();
        session.command(Command::Cancel { reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }
}
