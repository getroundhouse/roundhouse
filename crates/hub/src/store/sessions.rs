//! Sessions.

use protocol::model::{AgentKind, ClientRequestId, QueuedPrompt, Session, SessionStatus, Title};
use protocol::model::{SessionId, WorkspaceId};
use rusqlite::{OptionalExtension, Row, params};

use super::made::{Made, inserted, made_by};
use super::{Store, StoreError, StoredSession, from_text, lock, rev_of, text_of};

pub(super) const SESSION_COLUMNS: &str = "id, workspace_id, agent, model, permission_mode, status, last_seq, created_at_ms, updated_at_ms, agent_session_id, agent_opened, title, closed_at_ms, reply_seq, read_seq, background_tasks, title_source, queued, queue_held, turn_started_at_ms";

/// A session's queue, from its column.
pub(super) fn queue_from(json: &str) -> rusqlite::Result<Vec<QueuedPrompt>> {
    serde_json::from_str(json)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, Box::new(e)))
}

pub(super) fn session_from_row(row: &Row) -> rusqlite::Result<StoredSession> {
    Ok(StoredSession {
        session: Session {
            id: row.get::<_, String>(0)?.into(),
            workspace_id: row.get::<_, String>(1)?.into(),
            agent: from_text::<AgentKind>(row.get(2)?)?,
            model: row.get(3)?,
            permission_mode: row.get(4)?,
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
            rev: rev_of(row, 20)?,
        },
        agent_session_id: row.get::<_, String>(9)?.into(),
        agent_opened: row.get(10)?,
    })
}

impl Store {
    /// Returns the session as stored, with its revision.
    pub fn insert_session(
        &self,
        stored: &StoredSession,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Made<StoredSession>, StoreError> {
        let conn = lock(&self.writer);
        if let Some(made) = made_by(&conn, client_request_id)? {
            return Ok(Made::Earlier(made));
        }
        let session = &stored.session;
        conn.execute(
            &format!(
                "INSERT INTO sessions ({SESSION_COLUMNS}, client_request_id)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            params![
                session.id.as_str(),
                session.workspace_id.as_str(),
                text_of(&session.agent),
                session.model,
                session.permission_mode,
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
                client_request_id.map(ClientRequestId::as_str),
            ],
        )?;
        Ok(inserted(&conn, session.id.as_str())?)
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

    pub fn list_sessions(&self, workspace_id: &WorkspaceId) -> Result<Vec<Session>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {SESSION_COLUMNS}, rev FROM sessions WHERE workspace_id = ? ORDER BY created_at_ms"
        ))?;
        let rows = statement.query_map([workspace_id.as_str()], session_from_row)?;
        let sessions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sessions.into_iter().map(|s| s.session).collect())
    }

    /// `None` leaves that setting as it is.
    pub fn set_session_options(
        &self,
        id: &SessionId,
        model: Option<&str>,
        permission_mode: Option<&str>,
    ) -> Result<(), StoreError> {
        lock(&self.writer)
            .execute(
                "UPDATE sessions SET model = COALESCE(?, model), permission_mode = COALESCE(?, permission_mode) WHERE id = ?",
                params![model, permission_mode, id.as_str()],
            )
            ?;
        Ok(())
    }

    pub fn set_session_closed(&self, id: &SessionId, closed_at_ms: Option<i64>) -> Result<(), StoreError> {
        lock(&self.writer)
            .execute("UPDATE sessions SET closed_at_ms = ? WHERE id = ?", params![closed_at_ms, id.as_str()])?;
        Ok(())
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
