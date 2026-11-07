//! Sessions.

use protocol::model::{AgentKind, Session, SessionStatus};
use protocol::model::{SessionId, WorkspaceId};
use rusqlite::{OptionalExtension, Row, params};

use super::{Store, StoreError, StoredSession, from_text, lock, text_of};

const SESSION_COLUMNS: &str = "id, workspace_id, agent, model, permission_mode, status, last_seq, created_at_ms, updated_at_ms, agent_session_id, agent_opened, title, closed_at_ms";

fn session_from_row(row: &Row) -> rusqlite::Result<StoredSession> {
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
            title: row.get(11)?,
            closed_at_ms: row.get(12)?,
        },
        agent_session_id: row.get(9)?,
        agent_opened: row.get(10)?,
    })
}

impl Store {
    pub fn insert_session(&self, stored: &StoredSession) -> Result<(), StoreError> {
        let session = &stored.session;
        lock(&self.writer).execute(
            &format!("INSERT INTO sessions ({SESSION_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"),
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
                stored.agent_session_id,
                stored.agent_opened,
                session.title,
                session.closed_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &SessionId) -> Result<StoredSession, StoreError> {
        lock(&self.reader)
            .query_row(&format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?"), [id.as_str()], session_from_row)
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "session", id: id.to_string() })
    }

    pub fn list_sessions(&self, workspace_id: &WorkspaceId) -> Result<Vec<Session>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE workspace_id = ? ORDER BY created_at_ms"
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

    pub fn set_session_title(&self, id: &SessionId, title: &str) -> Result<(), StoreError> {
        lock(&self.writer).execute("UPDATE sessions SET title = ? WHERE id = ?", params![title, id.as_str()])?;
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

    /// Sessions that were mid-turn when the hub last stopped.
    pub fn active_session_ids(&self) -> Result<Vec<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT id FROM sessions WHERE status IN (?, ?)")?;
        let rows = statement
            .query_map([text_of(&SessionStatus::Running), text_of(&SessionStatus::WaitingApproval)], |row| {
                row.get::<_, String>(0).map(SessionId::from)
            })?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }
}
