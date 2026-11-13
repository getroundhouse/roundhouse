use super::super::{Hub, new_id};
use crate::error::internal;
use crate::session::{Command, LiveSession};
use crate::store::{Made, Places, Reading, Store, StoreError, StoredSession, now_ms};
use protocol::client::{ApiError, ErrorCode, HubNotice, log, records};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::find::{Found, Needle};
use protocol::model::{
    AgentKind, AnswerDraft, ApprovalId, AttachmentId, ClientMessageId, ClientRequestId, CompactionLine, Draft,
    DraftToKeep, Place, PromptLine, RecordKey, Rev, Seq, Session, SessionId, SessionSettings, SessionSettingsChange,
    SessionStatus, TitleSource, ToolUseId, WorkspaceId,
};
use protocol::prompt::Prompt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::warn;

/// The sessions the hub has in memory, and the ones it has deleted since it started.
#[derive(Default)]
pub(in super::super) struct Sessions {
    pub(in super::super) live: HashMap<SessionId, Arc<LiveSession>>,
    /// So a session read from the store just before it was deleted isn't brought back once it's gone.
    pub(super) deleted: HashSet<SessionId>,
}

impl Sessions {
    /// The one in memory, or else the one `start` makes from a copy just read from the store, unless the session has
    /// been deleted since.
    pub(in super::super) fn keep(
        &mut self,
        id: &SessionId,
        start: impl FnOnce() -> Arc<LiveSession>,
    ) -> Result<Arc<LiveSession>, ApiError> {
        if self.deleted.contains(id) {
            return Err(StoreError::NotFound { kind: "session", id: id.to_string() }.into());
        }
        Ok(self.live.entry(id.clone()).or_insert_with(start).clone())
    }

    /// Lets go of a session that's been deleted, and gives back what was in memory of it, if anything was.
    pub(super) fn forget(&mut self, id: &SessionId) -> Option<Arc<LiveSession>> {
        self.deleted.insert(id.clone());
        self.live.remove(id)
    }
}

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
        self.offered(agent, &options)?;
        self.ready_workspace(&workspace_id).await?;
        let id = new_id();
        let now = now_ms();
        let SessionSettings { model, permission_mode, effort } = self.agent(agent)?.defaults.clone().changed(options);
        let stored = StoredSession {
            session: Session {
                account: None,
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
            // The hub's ID doubles as the agent's, so `--resume` works after a crash even if the
            // agent never reported one.
            agent_session_id: id.into(),
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

    pub(crate) async fn session_ensure(
        self: &Arc<Self>,
        workspace_id: WorkspaceId,
        agent: AgentKind,
    ) -> Result<Session, ApiError> {
        let _one_at_a_time = self.ensuring_session.lock().await;
        let sessions = self.session_list(workspace_id.clone()).await?;
        match sessions.into_iter().rfind(Session::is_open) {
            Some(open) => Ok(open),
            None => self.session_create(workspace_id, agent, SessionSettingsChange::default(), None).await,
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
    ) -> Result<log::Events, ApiError> {
        let max_bytes = max_bytes.unwrap_or(crate::server::DEFAULT_READ_BYTES);
        let limit = limit.clamp(1, crate::server::MAX_READ_EVENTS);
        self.blocking(move |store| {
            let last_seq = store.get_session(&id)?.session.last_seq;
            let events = store.read_events(&id, from_seq, limit, max_bytes, include_raw)?;
            Ok(log::Events { events, last_seq })
        })
        .await
    }

    /// Every prompt in the session, as a line each. A session that isn't there is refused, as a read is.
    pub(crate) async fn session_prompt_list(&self, id: SessionId) -> Result<Vec<PromptLine>, ApiError> {
        self.blocking(move |store| {
            store.get_session(&id)?;
            store.prompt_lines(&id)
        })
        .await
    }

    /// Every compaction of the agent's own conversation that compacted, as a line each. A session that isn't there is
    /// refused, as a read is.
    pub(crate) async fn session_compaction_list(&self, id: SessionId) -> Result<Vec<CompactionLine>, ApiError> {
        self.blocking(move |store| {
            store.get_session(&id)?;
            store.compaction_lines(&id)
        })
        .await
    }

    /// Which of the agent's own events, or the subagent `subagent`'s, hold `query`. A session that isn't there is
    /// refused, as a read is, and so is a query with nothing to look for or too much.
    pub(crate) async fn session_find(
        &self,
        id: SessionId,
        subagent: Option<ToolUseId>,
        query: &str,
    ) -> Result<Found, ApiError> {
        let needle = Needle::new(query).map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))?;
        self.blocking(move |store| {
            store.get_session(&id)?;
            store.find_events(&id, subagent.as_ref(), &needle)
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
        let limit = u32::try_from(through.get() - from_seq.get() + 1)
            .map_or(crate::server::PUMP_PAGE_EVENTS, |n| n.min(crate::server::PUMP_PAGE_EVENTS));
        self.blocking(move |store| store.read_events(&id, from_seq, limit, crate::server::DEFAULT_READ_BYTES, false))
            .await
    }

    /// Reads the agent's own events, or those of the subagent `subagent` started, by place, as
    /// [`Hub::session_read`] reads every event by `seq`.
    pub(crate) async fn session_read_places(
        &self,
        id: SessionId,
        subagent: Option<ToolUseId>,
        from_place: Place,
        limit: u32,
        max_bytes: Option<u64>,
        include_raw: bool,
    ) -> Result<log::PlacedEvents, ApiError> {
        let max_bytes = max_bytes.unwrap_or(crate::server::DEFAULT_READ_BYTES);
        let limit = limit.clamp(1, crate::server::MAX_READ_EVENTS);
        self.blocking(move |store| {
            store.get_session(&id)?;
            let places = Places { subagent: subagent.as_ref(), from: from_place, through: None };
            let (events, last_place) = store.read_places(&id, places, limit, max_bytes, include_raw)?;
            Ok(log::PlacedEvents { events, last_place })
        })
        .await
    }

    /// Reads by place for a subscriber, as [`Hub::read_page`] does by `seq`, never past `through`.
    pub(crate) async fn read_places_page(
        &self,
        id: SessionId,
        subagent: Option<ToolUseId>,
        from: Place,
        through: Seq,
    ) -> Result<Vec<SessionEvent>, ApiError> {
        self.blocking(move |store| {
            let places = Places { subagent: subagent.as_ref(), from, through: Some(through) };
            let limit = crate::server::PUMP_PAGE_EVENTS;
            Ok(store.read_places(&id, places, limit, crate::server::DEFAULT_READ_BYTES, false)?.0)
        })
        .await
    }

    /// The session's subagents, oldest first, with the revision they're as of.
    pub(crate) async fn session_subagents(&self, id: SessionId) -> Result<records::Subagents, ApiError> {
        self.blocking(move |store| {
            store.get_session(&id)?;
            let (subagents, rev) = store.list_subagents(&id)?;
            Ok(records::Subagents { subagents, rev })
        })
        .await
    }

    pub(crate) async fn session_prompt(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
    ) -> Result<(Seq, bool), ApiError> {
        prompt.check()?;
        let session = self.live_session(&id).await?;
        let (reply, recorded) = oneshot::channel();
        session.command(Command::Prompt { client_msg_id, prompt, reply }).await?;
        recorded.await.map_err(|_| internal("the session has stopped"))?
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
        options: SessionSettingsChange,
    ) -> Result<Session, ApiError> {
        let session = self.live_session(&id).await?;
        self.offered(session.agent(), &options)?;
        // Not through the session's queue, because a prompt waiting on setup holds that. The choice
        // has to apply when that prompt finally starts the agent.
        session.configure(options).await
    }

    /// A chat nothing has been sent in has nothing to go back to, so closing it deletes it, along with any files added
    /// to it, which no prompt carried. The reply is then the session as it was, and the feed says it's gone. It's
    /// deleted before anything else, so nothing of it is brought into memory only to be let go.
    pub(crate) async fn session_close(self: &Arc<Self>, id: SessionId) -> Result<Session, ApiError> {
        let owned = id.clone();
        let Some((was, rev)) = self.blocking(move |store| store.delete_unused_session(&owned)).await? else {
            return self.live_session(&id).await?.close().await;
        };
        let forgotten = self.sessions.lock().forget(&id);
        self.notify(HubNotice::Removed { record: RecordKey::Session { id: id.clone() }, rev });
        if let Some(session) = forgotten {
            session.end().await;
        }
        // Gone whether or not its files are, so failing only leaves them on disk.
        if let Err(e) = self.attachment_files.remove_session(&id).await {
            warn!(session = %id, "{e}");
        }
        Ok(was)
    }

    pub(crate) async fn session_reopen(self: &Arc<Self>, id: SessionId) -> Result<Session, ApiError> {
        self.live_session(&id).await?.reopen().await
    }

    pub(crate) async fn session_mark_read(&self, id: SessionId, through_seq: Seq) -> Result<Session, ApiError> {
        self.change_session(move |store| store.mark_read(&id, through_seq)).await
    }

    pub(crate) async fn session_mark_unread(&self, id: SessionId) -> Result<Session, ApiError> {
        self.change_session(move |store| store.mark_unread(&id)).await
    }

    pub(crate) async fn session_draft(&self, id: SessionId, draft: DraftToKeep) -> Result<Session, ApiError> {
        self.change_session(move |store| store.set_draft(&id, &draft)).await
    }

    pub(crate) async fn session_answer_draft(
        &self,
        id: SessionId,
        draft: Option<AnswerDraft>,
    ) -> Result<Session, ApiError> {
        self.change_session(move |store| store.set_answer_draft(&id, draft.as_ref())).await
    }

    /// Changes something kept on a session that its agent doesn't need to hear of, such as how far it's read. Only
    /// the store's lock orders this against an agent's events, so this can announce its copy after one of theirs that's
    /// newer, which the feed then leaves out, as `Feed::send` says. Nothing is announced when nothing changed, since the
    /// revision didn't move.
    pub(super) async fn change_session(
        &self,
        change: impl FnOnce(&Store) -> Result<Reading, StoreError> + Send + 'static,
    ) -> Result<Session, ApiError> {
        let Reading { session, changed } = self.blocking(change).await?;
        if changed {
            self.notify(HubNotice::Session(session.session.clone()));
        }
        Ok(session.session)
    }

    pub(crate) async fn session_compact(
        self: &Arc<Self>,
        id: SessionId,
        keeping: Option<String>,
    ) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        self.compacts(session.agent())?;
        let (reply, done) = oneshot::channel();
        session.command(Command::Compact { keeping, reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
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
