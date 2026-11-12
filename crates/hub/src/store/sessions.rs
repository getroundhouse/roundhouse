//! Sessions.

use protocol::model::{
    AgentKind, ClientRequestId, ContextUsage, Draft, ModelName, PromptCache, QueuedPrompt, Rev, Session,
    SessionSettingsChange, SessionStatus, Title,
};
use protocol::model::{AttachmentId, RunId, SessionId, WorkspaceId};
use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use super::drafts::forget_drafts;
use super::made::{Made, inserted, made_by};
use super::{Store, StoreError, StoredSession, current_rev, from_text, lock, rev_of, text_of};

pub(super) const SESSION_COLUMNS: &str = "id, workspace_id, agent, model, permission_mode, status, last_seq, created_at_ms, updated_at_ms, agent_session_id, agent_opened, title, closed_at_ms, reply_seq, read_seq, background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id, user_active_at_ms, prompt_cache, draft, answer_draft";

/// A session's queue, from its column.
pub(super) fn queue_from(json: &str) -> rusqlite::Result<Vec<QueuedPrompt>> {
    serde_json::from_str(json)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, Box::new(e)))
}

/// A draft for its column: NULL when nothing is typed.
pub(super) fn draft_column(draft: &Draft) -> serde_json::Result<Option<String>> {
    (!draft.is_empty()).then(|| serde_json::to_string(draft)).transpose()
}

pub(super) fn session_from_row(row: &Row) -> rusqlite::Result<StoredSession> {
    Ok(StoredSession {
        session: Session {
            id: row.get::<_, String>(0)?.into(),
            workspace_id: row.get::<_, String>(1)?.into(),
            agent: from_text::<AgentKind>(row.get(2)?)?,
            model: from_text(row.get(3)?)?,
            permission_mode: from_text(row.get(4)?)?,
            status: from_text(row.get(5)?)?,
            last_seq: row.get::<_, i64>(6)?.cast_unsigned(),
            created_at_ms: row.get(7)?,
            updated_at_ms: row.get(8)?,
            title: row.get::<_, Option<String>>(11)?.as_deref().and_then(Title::tidied),
            closed_at_ms: row.get(12)?,
            reply_seq: row.get::<_, i64>(13)?.cast_unsigned(),
            read_seq: row.get::<_, i64>(14)?.cast_unsigned(),
            // Written only by this hub, as JSON. Anything it can't read is nothing it knows to be running.
            background_tasks: serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default(),
            title_source: from_text(row.get(16)?)?,
            queued: queue_from(&row.get::<_, String>(17)?)?,
            queue_held: row.get(18)?,
            turn_started_at_ms: row.get(19)?,
            effort: from_text(row.get(20)?)?,
            // Written only by this hub, as JSON. One it can't read is as good as none, until the agent next says.
            context: row.get::<_, Option<String>>(21)?.and_then(|json| serde_json::from_str(&json).ok()),
            account: row.get::<_, Option<String>>(22)?.map(Into::into),
            user_active_at_ms: row.get(23)?,
            // Written only by this hub, as JSON. One it can't read is as good as none, until the agent next asks.
            prompt_cache: row.get::<_, Option<String>>(24)?.and_then(|json| serde_json::from_str(&json).ok()),
            // Written only by this hub, as JSON. One it can't read is as good as nothing typed.
            draft: row
                .get::<_, Option<String>>(25)?
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default(),
            // The same.
            answer_draft: row.get::<_, Option<String>>(26)?.and_then(|json| serde_json::from_str(&json).ok()),
            rev: rev_of(row, 27)?,
        },
        agent_session_id: row.get::<_, String>(9)?.into(),
        agent_opened: row.get(10)?,
    })
}

impl Store {
    /// Returns the session as stored, with its revision. Making it is when the person last did something in its
    /// workspace, which moves in the same commit.
    pub fn insert_session(
        &self,
        stored: &StoredSession,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Made<StoredSession>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        if let Some(made) = made_by(&tx, client_request_id)? {
            return Ok(Made::Earlier(made));
        }
        let session = &stored.session;
        tx.execute(
            &format!(
                "INSERT INTO sessions ({SESSION_COLUMNS}, client_request_id)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                session.id.as_str(),
                session.workspace_id.as_str(),
                text_of(&session.agent),
                session.model.as_str(),
                text_of(&session.permission_mode),
                text_of(&session.status),
                session.last_seq.cast_signed(),
                session.created_at_ms,
                session.updated_at_ms,
                stored.agent_session_id.as_str(),
                stored.agent_opened,
                session.title.as_deref(),
                session.closed_at_ms,
                session.reply_seq.cast_signed(),
                session.read_seq.cast_signed(),
                serde_json::to_string(&session.background_tasks)?,
                text_of(&session.title_source),
                serde_json::to_string(&session.queued)?,
                session.queue_held,
                session.turn_started_at_ms,
                text_of(&session.effort),
                session.context.as_ref().map(serde_json::to_string).transpose()?,
                session.account.as_ref().map(protocol::model::AccountId::as_str),
                session.user_active_at_ms,
                session.prompt_cache.as_ref().map(serde_json::to_string).transpose()?,
                draft_column(&session.draft)?,
                session.answer_draft.as_ref().map(serde_json::to_string).transpose()?,
                client_request_id.map(ClientRequestId::as_str),
            ],
        )?;
        // Making a chat is doing something in its workspace.
        tx.execute(
            "UPDATE workspaces SET user_active_at_ms = ? WHERE id = ?",
            params![session.created_at_ms, session.workspace_id.as_str()],
        )?;
        let made = inserted(&tx, session.id.as_str())?;
        tx.commit()?;
        Ok(made)
    }

    pub fn get_session(&self, id: &SessionId) -> Result<StoredSession, StoreError> {
        lock(&self.reader)
            .query_row(
                &format!("SELECT {SESSION_COLUMNS}, rev FROM sessions WHERE id = ?"),
                [id.as_str()],
                session_from_row,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "session", id: id.to_string() })
    }

    /// The run whose worker the session last recorded something from that the worker numbered, and its number.
    pub fn recorded_from_worker(&self, id: &SessionId) -> Result<Option<(RunId, u64)>, StoreError> {
        let found: Option<(Option<i64>, Option<i64>)> = lock(&self.reader)
            .query_row("SELECT recorded_run, recorded_through FROM sessions WHERE id = ?", [id.as_str()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        Ok(match found {
            Some((Some(run), Some(through))) => Some((RunId::new(run.cast_unsigned()), through.cast_unsigned())),
            _ => None,
        })
    }

    pub fn list_sessions(&self, workspace_id: &WorkspaceId) -> Result<Vec<Session>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {SESSION_COLUMNS}, rev FROM sessions WHERE workspace_id = ? ORDER BY created_at_ms"
        ))?;
        let rows = statement.query_map([workspace_id.as_str()], session_from_row)?;
        let sessions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sessions.into_iter().map(|s| s.session).collect())
    }

    /// A setting that's `None` is left as it is.
    pub fn set_session_options(&self, id: &SessionId, options: &SessionSettingsChange) -> Result<(), StoreError> {
        let SessionSettingsChange { model, permission_mode, effort } = options;
        lock(&self.writer).execute(
            "UPDATE sessions
                 SET model = COALESCE(?, model), permission_mode = COALESCE(?, permission_mode),
                     effort = COALESCE(?, effort)
                 WHERE id = ?",
            params![
                model.as_ref().map(ModelName::as_str),
                permission_mode.as_ref().map(text_of),
                effort.as_ref().map(text_of),
                id.as_str()
            ],
        )?;
        Ok(())
    }

    /// How full the session's agent's context is from here on.
    pub fn set_session_context(&self, id: &SessionId, context: &ContextUsage) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            "UPDATE sessions SET context = ? WHERE id = ?",
            params![serde_json::to_string(context)?, id.as_str()],
        )?;
        Ok(())
    }

    /// What the API's cache holds of the session's conversation from here on, or `None` once it holds none of it.
    pub fn set_session_cache(&self, id: &SessionId, cache: Option<&PromptCache>) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            "UPDATE sessions SET prompt_cache = ? WHERE id = ?",
            params![cache.map(serde_json::to_string).transpose()?, id.as_str()],
        )?;
        Ok(())
    }

    /// Closes the session, or opens it again with `None`. Its draft goes as it closes, with its composer, and so do any
    /// answer being typed to its question, and each file the draft named that nothing else will send, which this
    /// returns, so the caller lets go of their bytes too. One transaction, so a draft kept meanwhile either comes first
    /// and goes too, or finds the session closed.
    pub fn set_session_closed(
        &self,
        id: &SessionId,
        closed_at_ms: Option<i64>,
    ) -> Result<Vec<AttachmentId>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE sessions SET closed_at_ms = ? WHERE id = ?", params![closed_at_ms, id.as_str()])?;
        let let_go_of = if closed_at_ms.is_some() { forget_drafts(&tx, id)? } else { Vec::new() };
        tx.commit()?;
        Ok(let_go_of)
    }

    /// Deletes a session nothing has been sent in, with nothing in its log and nothing queued, along with what it
    /// knows of any files added to it, which no prompt carried. Returns it as it was, and the revision deleting it
    /// made, or `None`, leaving it be, once anything has been sent. One transaction, so a prompt being recorded either
    /// comes first and keeps the session, or finds it gone.
    pub fn delete_unused_session(&self, id: &SessionId) -> Result<Option<(Session, Rev)>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(stored) = tx
            .query_row(
                &format!("SELECT {SESSION_COLUMNS}, rev FROM sessions WHERE id = ?"),
                [id.as_str()],
                session_from_row,
            )
            .optional()?
        else {
            return Err(StoreError::NotFound { kind: "session", id: id.to_string() });
        };
        let logged: bool =
            tx.query_row("SELECT EXISTS (SELECT 1 FROM events WHERE session_id = ?)", [id.as_str()], |row| row.get(0))?;
        if logged || !stored.session.queued.is_empty() {
            return Ok(None);
        }
        tx.execute("DELETE FROM attachments WHERE session_id = ?", [id.as_str()])?;
        tx.execute("DELETE FROM sessions WHERE id = ?", [id.as_str()])?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok(Some((stored.session, rev)))
    }

    pub fn set_agent_opened(&self, id: &SessionId) -> Result<(), StoreError> {
        lock(&self.writer).execute("UPDATE sessions SET agent_opened = 1 WHERE id = ?", [id.as_str()])?;
        Ok(())
    }

    /// Sessions whose agent was at work when the hub last stopped, in a turn or in the background.
    pub fn active_session_ids(&self) -> Result<Vec<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT id, status FROM sessions")?;
        let rows = statement.query_map([], |row| {
            Ok((SessionId::from(row.get::<_, String>(0)?), from_text::<SessionStatus>(row.get(1)?)?))
        })?;
        let mut active = Vec::new();
        for row in rows {
            let (id, status) = row?;
            if status.is_active() {
                active.push(id);
            }
        }
        Ok(active)
    }
}

#[cfg(test)]
mod tests;
