//! The session requests, most of which pass straight to the live session.

use std::sync::Arc;

use protocol::attachment::MAX_PER_PROMPT;
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubNotice};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::model::{
    AgentKind, ApprovalId, AttachmentId, ClientMessageId, ClientRequestId, Rev, Seq, Session, SessionId, SessionStatus,
    TitleSource, WorkspaceId, WorkspaceStatus,
};
use tokio::sync::oneshot;

use super::{Hub, new_id, not_ready};
use crate::error::internal;
use crate::session::Command;
use crate::store::{Made, Reading, Store, StoreError, StoredSession, now_ms};

impl Hub {
    pub(crate) async fn session_create(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session, ApiError> {
        if let Some(made) = self.made_earlier::<StoredSession>(client_request_id.as_ref()).await? {
            return Ok(made.session);
        }
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
                id: id.clone().into(),
                workspace_id,
                agent,
                model,
                permission_mode,
                status: SessionStatus::Idle,
                last_seq: 0,
                created_at_ms: now,
                updated_at_ms: now,
                title: None,
                title_source: TitleSource::FirstLine,
                closed_at_ms: None,
                reply_seq: 0,
                read_seq: 0,
                turn_started_at_ms: None,
                background_tasks: Vec::new(),
                queued: Vec::new(),
                queue_held: false,
                rev: Rev::default(),
            },
            // The hub's ID doubles as the agent's, so `--resume` works after a crash even if the
            // agent never reported one.
            agent_session_id: id.into(),
            agent_opened: false,
        };
        match self.blocking(move |store| store.insert_session(&stored, client_request_id.as_ref())).await? {
            Made::New(made) => {
                self.notify(HubNotice::Session(made.session.clone()));
                Ok(made.session)
            }
            Made::Earlier(made) => Ok(made.session),
        }
    }

    pub(crate) async fn session_ensure(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        agent: AgentKind,
    ) -> Result<Session, ApiError> {
        let _one_at_a_time = self.ensuring_session.lock().await;
        let sessions = self.session_list(workspace_id.clone()).await?;
        match sessions.into_iter().rfind(|s| s.closed_at_ms.is_none()) {
            Some(open) => Ok(open),
            None => self.session_create(workspace_id, agent, None, None, None).await,
        }
    }

    pub(crate) async fn session_list(&self, workspace_id: WorkspaceId) -> Result<Vec<Session>, ApiError> {
        self.blocking(move |store| store.list_sessions(&workspace_id)).await
    }

    pub(crate) async fn session_get(&self, id: SessionId) -> Result<Session, ApiError> {
        self.blocking(move |store| store.get_session(&id).map(|stored| stored.session)).await
    }

    pub(crate) async fn session_read(
        &self,
        id: SessionId,
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
        id: SessionId,
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
        id: SessionId,
        text: String,
        client_msg_id: Option<ClientMessageId>,
        attachments: Vec<AttachmentId>,
    ) -> Result<(Seq, bool), ApiError> {
        if text.trim().is_empty() && attachments.is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "a prompt can't be empty"));
        }
        if attachments.len() > MAX_PER_PROMPT {
            let message = format!("a prompt can carry at most {MAX_PER_PROMPT} files");
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        let session = self.live_session(&id).await?;
        let (reply, recorded) = oneshot::channel();
        session.command(Command::Prompt { text, client_msg_id, attachments, reply }).await?;
        recorded.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_queue(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
    ) -> Result<Session, ApiError> {
        self.live_session(&id).await?.queue(client_msg_id, text).await
    }

    pub(crate) async fn session_unqueue(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
    ) -> Result<Session, ApiError> {
        self.live_session(&id).await?.unqueue(client_msg_id).await
    }

    pub(crate) async fn session_send_queued(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
    ) -> Result<Session, ApiError> {
        let session = self.live_session(&id).await?;
        let (reply, sent) = oneshot::channel();
        session.command(Command::SendQueuedNow { client_msg_id, reply }).await?;
        sent.await.map_err(|_| internal("the session has stopped"))?
    }

    /// At startup, starts each session with a queue, so a queue that was ready to send when the last hub went away is
    /// sent now, rather than once someone opens its chat.
    pub(crate) async fn start_queues(self: &Arc<Self>) {
        let ids = match self.blocking(Store::sessions_with_queues).await {
            Ok(ids) => ids,
            Err(e) => return tracing::error!("couldn't read the queues: {e}"),
        };
        for id in ids {
            if let Err(e) = self.live_session(&id).await {
                tracing::error!(session = %id, "couldn't start a session with a queue: {e}");
            }
        }
    }

    pub(crate) async fn session_approve(
        self: &Arc<Self>,
        id: SessionId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    ) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        let (reply, done) = oneshot::channel();
        session.command(Command::Approve { approval_id, decision, reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_configure(
        self: &Arc<Self>,
        id: SessionId,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session, ApiError> {
        // Not through the session's queue, because a prompt waiting on setup holds that. The choice
        // has to apply when that prompt finally starts the agent.
        self.live_session(&id).await?.configure(model, permission_mode).await
    }

    pub(crate) async fn session_close(self: &Arc<Self>, id: SessionId) -> Result<Session, ApiError> {
        self.live_session(&id).await?.close().await
    }

    pub(crate) async fn session_reopen(self: &Arc<Self>, id: SessionId) -> Result<Session, ApiError> {
        self.live_session(&id).await?.reopen().await
    }

    pub(crate) async fn session_mark_read(&self, id: SessionId, through_seq: Seq) -> Result<Session, ApiError> {
        self.change_reading(move |store| store.mark_read(&id, through_seq)).await
    }

    pub(crate) async fn session_mark_unread(&self, id: SessionId) -> Result<Session, ApiError> {
        self.change_reading(move |store| store.mark_unread(&id)).await
    }

    /// Changes how far a session is read. Only the store's lock orders this against an agent's events, which needs
    /// nothing more, as each reads the session back with the revision its change gave it. Nothing is announced when
    /// nothing changed, since the revision didn't move.
    async fn change_reading(
        &self,
        change: impl FnOnce(&Store) -> Result<Reading, StoreError> + Send + 'static,
    ) -> Result<Session, ApiError> {
        let Reading { session, changed } = self.blocking(change).await?;
        if changed {
            self.notify(HubNotice::Session(session.session.clone()));
        }
        Ok(session.session)
    }

    pub(crate) async fn session_cancel(self: &Arc<Self>, id: SessionId) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        // A prompt waiting on setup holds this session's commands, so stopping has to reach setup directly.
        self.cancel_setup(&session.workspace_id);
        let (reply, done) = oneshot::channel();
        session.command(Command::Cancel { reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }
}
