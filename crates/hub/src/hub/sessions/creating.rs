//! Making a chat: what it runs with, as its agent offers where it runs, and its record.

use super::super::{Hub, new_id};
use crate::store::{Made, StoredSession, now_ms};
use protocol::client::{ApiError, HubNotice};
use protocol::model::{
    AgentKind, ClientRequestId, Draft, Rev, Seq, Session, SessionSettings, SessionSettingsChange, SessionStatus,
    TitleSource, WorkspaceId,
};
use std::sync::Arc;

impl Hub {
    pub(crate) async fn session_create(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        agent: AgentKind,
        options: SessionSettingsChange,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session, ApiError> {
        if let Some(made) = self.made_earlier::<StoredSession>(client_request_id.as_ref()).await? {
            return Ok(made.session);
        }
        let settings = self.initial_settings(agent, workspace_id.clone(), options).await?;
        self.insert_session(workspace_id, agent, settings, client_request_id).await
    }

    /// Records a new chat in `workspace_id` that runs with `settings`, which its agent has been offered.
    async fn insert_session(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        agent: AgentKind,
        settings: SessionSettings,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session, ApiError> {
        self.ready_workspace(&workspace_id).await?;
        let id = new_id();
        let now = now_ms();
        let SessionSettings { model, permission_mode, effort } = settings;
        let stored = StoredSession {
            session: Session {
                account: None,
                account_source: None,
                agent_capabilities: None,
                reply_outcome: None,
                id: id.clone().into(),
                workspace_id,
                agent,
                model,
                permission_mode,
                effort,
                status: SessionStatus::Idle,
                last_seq: Seq::default(),
                created_at_ms: now,
                updated_at_ms: now,
                title: None,
                title_source: TitleSource::FirstLine,
                closed_at_ms: None,
                reply_seq: Seq::default(),
                read_seq: Seq::default(),
                turn_started_at_ms: None,
                user_active_at_ms: None,
                background_tasks: Vec::new(),
                wait: None,
                waits: Some(Vec::new()),
                context: None,
                prompt_cache: None,
                queued: Vec::new(),
                queue_held: false,
                draft: Draft::default(),
                answer_draft: None,
                open_approvals: Some(Vec::new()),
                untaken: Some(Vec::new()),
                agent_opened: Some(false),
                rev: Rev::default(),
            },
            agent_session_id: self.new_conversation_id(agent, &id)?,
            agent_opened: false,
        };
        match self.blocking(move |store| store.insert_session(&stored, client_request_id.as_ref())).await? {
            Made::New(made) => {
                self.notify(HubNotice::Session(made.session.clone()));
                // Making it moved when the person last did something in its workspace.
                self.announce_workspace(&made.session.workspace_id).await;
                Ok(made.session)
            }
            Made::Earlier(made) => Ok(made.session),
        }
    }

    fn new_conversation_id(
        &self,
        agent: AgentKind,
        id: &str,
    ) -> Result<Option<protocol::model::AgentSessionId>, ApiError> {
        let claims = self.agent(agent)?.capabilities.as_ref();
        let named = claims.map_or(agent == AgentKind::Claude, |claims| {
            matches!(
                claims.conversation.resume.as_ref().map(|resume| &resume.ids),
                Some(protocol::capability::conversation::ConversationIds::Given)
            )
        });
        Ok(named.then(|| id.into()))
    }

    pub(crate) async fn session_ensure(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        agent: AgentKind,
    ) -> Result<Session, ApiError> {
        let open = |sessions: Vec<Session>| sessions.into_iter().rfind(Session::is_open);
        if let Some(open) = open(self.session_list(workspace_id.clone()).await?) {
            return Ok(open);
        }
        // Before the lock, which every workspace's first chat takes, since the agent may be asked what it offers.
        let settings = self.initial_settings(agent, workspace_id.clone(), SessionSettingsChange::default()).await?;
        let _one_at_a_time = self.ensuring_session.lock().await;
        match open(self.session_list(workspace_id.clone()).await?) {
            Some(open) => Ok(open),
            None => self.insert_session(workspace_id, agent, settings, None).await,
        }
    }
}
