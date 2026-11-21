use super::super::Hub;
use crate::error::internal;
use crate::session::{Command, LiveSession};
use crate::store::{Places, Reading, Store, StoreError};
use protocol::capability::approvals::Decision;
use protocol::capability::settings::Settings;
use protocol::client::log::ReadLimit;
use protocol::client::{ApiError, ErrorCode, HubNotice, log, records};
use protocol::event::SessionEvent;
use protocol::find::{Found, Needle};
use protocol::model::{
    ApprovalId, AttachmentId, ClientDevice, ClientMessageId, CompactionLine, DecisionDraft, DraftToKeep, Ending,
    MessageId, Place, PromptLine, RecordKey, Seq, Session, SessionId, SubagentId, WorkspaceId,
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
        let read = ReadLimit::asked(limit, max_bytes);
        self.blocking(move |store| {
            let last_seq = store.get_session(&id)?.session.last_seq;
            let events = store.read_events(&id, from_seq, read.events, read.bytes, include_raw)?;
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
        subagent: Option<SubagentId>,
        query: &str,
    ) -> Result<Found, ApiError> {
        let needle = Needle::new(query).map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))?;
        self.blocking(move |store| {
            store.get_session(&id)?;
            let subagent = subagent.map(|named| store.subagent_named(&id, &named)).transpose()?;
            store.find_events(&id, subagent.as_ref(), &needle)
        })
        .await
    }

    /// The subagent `named` names in the session `id`, as [`Store::recorded_subagent`] finds it, if there's a record
    /// of it.
    pub(crate) async fn recorded_subagent(
        &self,
        id: SessionId,
        named: SubagentId,
    ) -> Result<Option<SubagentId>, ApiError> {
        self.blocking(move |store| store.recorded_subagent(&id, &named)).await
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
        self.blocking(move |store| store.read_events(&id, from_seq, limit, ReadLimit::BYTES, false)).await
    }

    /// Reads the agent's own events, or the subagent `subagent`'s, by place, as [`Hub::session_read`] reads every event
    /// by `seq`.
    pub(crate) async fn session_read_places(
        &self,
        id: SessionId,
        subagent: Option<SubagentId>,
        from_place: Place,
        limit: u32,
        max_bytes: Option<u64>,
        include_raw: bool,
    ) -> Result<log::PlacedEvents, ApiError> {
        let read = ReadLimit::asked(limit, max_bytes);
        self.blocking(move |store| {
            store.get_session(&id)?;
            let subagent = subagent.map(|named| store.subagent_named(&id, &named)).transpose()?;
            let places = Places { subagent: subagent.as_ref(), from: from_place, through: None };
            let (events, last_place) = store.read_places(&id, places, read.events, read.bytes, include_raw)?;
            Ok(log::PlacedEvents { events, last_place })
        })
        .await
    }

    /// Reads by place for a subscriber, as [`Hub::read_page`] does by `seq`, never past `through`.
    pub(crate) async fn read_places_page(
        &self,
        id: SessionId,
        subagent: Option<SubagentId>,
        from: Place,
        through: Seq,
    ) -> Result<Vec<SessionEvent>, ApiError> {
        self.blocking(move |store| {
            let places = Places { subagent: subagent.as_ref(), from, through: Some(through) };
            let limit = crate::server::PUMP_PAGE_EVENTS;
            Ok(store.read_places(&id, places, limit, ReadLimit::BYTES, false)?.0)
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
        device: Option<ClientDevice>,
    ) -> Result<(Seq, bool), ApiError> {
        prompt.check()?;
        let session = self.live_session(&id).await?;
        let (reply, recorded) = oneshot::channel();
        session.command(Command::Prompt { client_msg_id, prompt, device, reply }).await?;
        recorded.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_decide(
        self: &Arc<Self>,
        id: SessionId,
        approval_id: ApprovalId,
        decision: Decision,
    ) -> Result<(), ApiError> {
        let session = self.live_session(&id).await?;
        let (reply, done) = oneshot::channel();
        session.command(Command::Decide { approval_id, decision, reply }).await?;
        done.await.map_err(|_| internal("the session has stopped"))?
    }

    pub(crate) async fn session_configure(
        self: &Arc<Self>,
        id: SessionId,
        change: Settings,
    ) -> Result<Session, ApiError> {
        crate::session::ChatSettings::keeps(&change)?;
        let session = self.live_session(&id).await?;
        // Not through the session's queue, because a prompt waiting on setup holds that. The choice
        // has to apply when that prompt finally starts the agent.
        session.configure(change).await
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

    /// Turns conversation mode on in the chat `id`, with the conversation on `device`, or moves it there.
    pub(crate) async fn session_conversation_start(
        self: &Arc<Self>,
        id: SessionId,
        device: ClientDevice,
    ) -> Result<Session, ApiError> {
        self.live_session(&id).await?.start_conversation(device).await
    }

    pub(crate) async fn session_conversation_end(self: &Arc<Self>, id: SessionId) -> Result<Session, ApiError> {
        self.live_session(&id).await?.end_conversation().await
    }

    pub(crate) async fn session_reply_heard(
        self: &Arc<Self>,
        id: SessionId,
        device: Option<&ClientDevice>,
        message: MessageId,
        ending: Ending,
    ) -> Result<(), ApiError> {
        self.live_session(&id).await?.keep_reply_heard(device, message, ending).await
    }

    pub(crate) async fn session_replies_heard(&self, id: SessionId) -> Result<records::RepliesHeard, ApiError> {
        self.blocking(move |store| {
            store.get_session(&id)?;
            let (replies, rev) = store.list_replies_heard(&id)?;
            Ok(records::RepliesHeard { replies, rev })
        })
        .await
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

    pub(crate) async fn session_decision_draft(
        &self,
        id: SessionId,
        draft: Option<DecisionDraft>,
    ) -> Result<Session, ApiError> {
        self.change_session(move |store| store.set_decision_draft(&id, draft.as_ref())).await
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
