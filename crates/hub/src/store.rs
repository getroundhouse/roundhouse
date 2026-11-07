//! SQLite storage. Synchronous, and only called through `spawn_blocking`.
//!
//! All SQL lives in this file, so it's the one thing a Postgres-backed hub would replace.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, MutexGuard};
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{
    AgentKind, DiffComment, Repo, Seq, Session, SessionStatus, SetupStatus, Workspace, WorkspaceStatus,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Applied in order. `PRAGMA user_version` records how many have run.
const MIGRATIONS: &[&str] = &[
    r"
    CREATE TABLE repos (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        created_at_ms INTEGER NOT NULL
    );
    CREATE TABLE workspaces (
        id TEXT PRIMARY KEY,
        repo_id TEXT NOT NULL REFERENCES repos(id),
        name TEXT NOT NULL,
        branch TEXT NOT NULL,
        base_ref TEXT,
        path TEXT,
        status TEXT NOT NULL,
        provider TEXT NOT NULL,
        instance_id TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        archived_at_ms INTEGER,
        UNIQUE (repo_id, name)
    );
    CREATE TABLE sessions (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        agent TEXT NOT NULL,
        model TEXT,
        permission_mode TEXT,
        agent_session_id TEXT NOT NULL,
        agent_opened INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL,
        last_seq INTEGER NOT NULL DEFAULT 0,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX sessions_by_workspace ON sessions (workspace_id, created_at_ms);
    -- raw is last, and SQLite reads columns lazily, so range reads don't touch it.
    CREATE TABLE events (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        seq INTEGER NOT NULL,
        ts_ms INTEGER NOT NULL,
        kind TEXT NOT NULL,
        client_msg_id TEXT,
        body TEXT NOT NULL,
        raw TEXT,
        PRIMARY KEY (session_id, seq)
    );
    CREATE UNIQUE INDEX events_client_msg ON events (session_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
    CREATE INDEX events_by_kind ON events (session_id, kind);
",
    r"
    CREATE TABLE diff_comments (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        path TEXT NOT NULL,
        side TEXT NOT NULL,
        line INTEGER NOT NULL,
        text TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
    CREATE INDEX diff_comments_by_workspace ON diff_comments (workspace_id, created_at_ms);
",
    r#"
    ALTER TABLE workspaces ADD COLUMN setup TEXT NOT NULL DEFAULT '{"state":"not_started"}';
    ALTER TABLE workspaces ADD COLUMN port_base INTEGER NOT NULL DEFAULT 0;
"#,
    r"
    ALTER TABLE sessions ADD COLUMN title TEXT;
    ALTER TABLE sessions ADD COLUMN closed_at_ms INTEGER;
",
];

const FIRST_PORT_BASE: i64 = 55_000;
const PORTS_PER_WORKSPACE: i64 = 10;

pub struct Store {
    /// One writer, which is what gives each session a gapless sequence.
    writer: Mutex<Connection>,
    /// A separate connection, so a large read doesn't delay an agent's events. WAL allows this.
    reader: Mutex<Connection>,
}

/// A session record, plus the fields only the hub needs.
#[derive(Clone, Debug)]
pub struct StoredSession {
    pub session: Session,
    pub agent_session_id: String,
    /// Whether the agent has opened this conversation, which decides how it's launched next.
    pub agent_opened: bool,
}

pub struct Appended {
    pub event: SessionEvent,
    /// This `client_msg_id` was already recorded, and `event` is the original.
    pub duplicate: bool,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn lock(conn: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    conn.lock()
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::Internal, e.to_string())
}

/// Enum to its `snake_case` serde name, for storing in a text column.
fn text_of<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        _ => unreachable!("only used with unit enums"),
    }
}

fn from_text<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_value(Value::String(text))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn repo_from_row(row: &Row) -> rusqlite::Result<Repo> {
    Ok(Repo { id: row.get(0)?, name: row.get(1)?, path: row.get(2)?, created_at_ms: row.get(3)? })
}

const WORKSPACE_COLUMNS: &str = "id, repo_id, name, branch, base_ref, path, status, provider, instance_id, created_at_ms, archived_at_ms, setup, port_base";

fn workspace_from_row(row: &Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        name: row.get(2)?,
        branch: row.get(3)?,
        base_ref: row.get(4)?,
        path: row.get(5)?,
        status: from_text(row.get(6)?)?,
        provider: row.get(7)?,
        instance_id: row.get(8)?,
        created_at_ms: row.get(9)?,
        archived_at_ms: row.get(10)?,
        setup: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or(SetupStatus::Unknown),
        port_base: row.get::<_, i64>(12)? as u16,
    })
}

const SESSION_COLUMNS: &str = "id, workspace_id, agent, model, permission_mode, status, last_seq, created_at_ms, updated_at_ms, agent_session_id, agent_opened, title, closed_at_ms";

fn session_from_row(row: &Row) -> rusqlite::Result<StoredSession> {
    Ok(StoredSession {
        session: Session {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            agent: from_text::<AgentKind>(row.get(2)?)?,
            model: row.get(3)?,
            permission_mode: row.get(4)?,
            status: from_text(row.get(5)?)?,
            last_seq: row.get::<_, i64>(6)? as Seq,
            created_at_ms: row.get(7)?,
            updated_at_ms: row.get(8)?,
            title: row.get(11)?,
            closed_at_ms: row.get(12)?,
        },
        agent_session_id: row.get(9)?,
        agent_opened: row.get(10)?,
    })
}

fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // With WAL this can lose the last few commits in a power cut, but not corrupt the database.
    // The agent's own session file holds the conversation regardless.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let mut writer = open_connection(path)?;
        migrate(&mut writer)?;
        let reader = open_connection(path)?;
        Ok(Self { writer: Mutex::new(writer), reader: Mutex::new(reader) })
    }

    // Repos ---------------------------------------------------------------------------------------

    pub fn add_repo(&self, repo: &Repo) -> Result<Repo, ApiError> {
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO repos (id, name, path, created_at_ms) VALUES (?, ?, ?, ?) ON CONFLICT (path) DO NOTHING",
            params![repo.id, repo.name, repo.path, repo.created_at_ms],
        )
        .map_err(internal)?;
        // Adding a repository twice returns the first record.
        conn.query_row("SELECT id, name, path, created_at_ms FROM repos WHERE path = ?", [&repo.path], repo_from_row)
            .map_err(internal)
    }

    pub fn list_repos(&self) -> Result<Vec<Repo>, ApiError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached("SELECT id, name, path, created_at_ms FROM repos ORDER BY name").map_err(internal)?;
        let rows = statement.query_map([], repo_from_row).map_err(internal)?;
        rows.collect::<rusqlite::Result<_>>().map_err(internal)
    }

    pub fn get_repo(&self, id: &str) -> Result<Repo, ApiError> {
        lock(&self.reader)
            .query_row("SELECT id, name, path, created_at_ms FROM repos WHERE id = ?", [id], repo_from_row)
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no repository {id}")))
    }

    // Workspaces ----------------------------------------------------------------------------------

    /// Returns false if the name is taken in this repository.
    pub fn insert_workspace(&self, workspace: &Workspace) -> Result<bool, ApiError> {
        let inserted = lock(&self.writer)
            .execute(
                &format!(
                    "INSERT INTO workspaces ({WORKSPACE_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT (repo_id, name) DO NOTHING"
                ),
                params![
                    workspace.id,
                    workspace.repo_id,
                    workspace.name,
                    workspace.branch,
                    workspace.base_ref,
                    workspace.path,
                    text_of(&workspace.status),
                    workspace.provider,
                    workspace.instance_id,
                    workspace.created_at_ms,
                    workspace.archived_at_ms,
                    serde_json::to_string(&workspace.setup).map_err(internal)?,
                    i64::from(workspace.port_base),
                ],
            )
            .map_err(internal)?;
        Ok(inserted == 1)
    }

    pub fn delete_workspace(&self, id: &str) -> Result<(), ApiError> {
        lock(&self.writer).execute("DELETE FROM workspaces WHERE id = ?", [id]).map_err(internal)?;
        Ok(())
    }

    pub fn workspace_ready(&self, id: &str, path: &str, branch: &str, base_ref: &str) -> Result<Workspace, ApiError> {
        lock(&self.writer)
            .execute(
                "UPDATE workspaces SET path = ?, branch = ?, base_ref = ?, status = ? WHERE id = ?",
                params![path, branch, base_ref, text_of(&WorkspaceStatus::Ready), id],
            )
            .map_err(internal)?;
        self.get_workspace(id)
    }

    pub fn set_workspace_archived(&self, id: &str, archived: bool) -> Result<Workspace, ApiError> {
        let (status, archived_at) =
            if archived { (WorkspaceStatus::Archived, Some(now_ms())) } else { (WorkspaceStatus::Ready, None) };
        lock(&self.writer)
            .execute(
                "UPDATE workspaces SET status = ?, archived_at_ms = ? WHERE id = ?",
                params![text_of(&status), archived_at, id],
            )
            .map_err(internal)?;
        self.get_workspace(id)
    }

    pub fn set_workspace_setup(&self, id: &str, setup: SetupStatus) -> Result<Workspace, ApiError> {
        lock(&self.writer)
            .execute(
                "UPDATE workspaces SET setup = ? WHERE id = ?",
                params![serde_json::to_string(&setup).map_err(internal)?, id],
            )
            .map_err(internal)?;
        self.get_workspace(id)
    }

    /// The start of ten ports nothing else has. Never reused, so a restored workspace keeps its own.
    pub fn next_port_base(&self) -> Result<u16, ApiError> {
        let highest: i64 = lock(&self.reader)
            .query_row("SELECT COALESCE(MAX(port_base), 0) FROM workspaces", [], |row| row.get(0))
            .map_err(internal)?;
        let next = if highest == 0 { FIRST_PORT_BASE } else { highest + PORTS_PER_WORKSPACE };
        // Past the top of the range, start again rather than fail. A clash is possible, but only
        // after thousands of workspaces, and it only matters if both are running a server.
        Ok(if next + PORTS_PER_WORKSPACE > 65_000 { FIRST_PORT_BASE as u16 } else { next as u16 })
    }

    /// A setup that was running when the hub stopped never finished, and its worker has gone.
    pub fn fail_interrupted_setups(&self) -> Result<(), ApiError> {
        let running = serde_json::to_string(&SetupStatus::Running).map_err(internal)?;
        let failed = serde_json::to_string(&SetupStatus::Failed { code: None }).map_err(internal)?;
        lock(&self.writer)
            .execute("UPDATE workspaces SET setup = ? WHERE setup = ?", params![failed, running])
            .map_err(internal)?;
        Ok(())
    }

    pub fn set_workspace_branch(&self, id: &str, branch: &str) -> Result<(), ApiError> {
        lock(&self.writer)
            .execute("UPDATE workspaces SET branch = ? WHERE id = ?", params![branch, id])
            .map_err(internal)?;
        Ok(())
    }

    pub fn get_workspace(&self, id: &str) -> Result<Workspace, ApiError> {
        lock(&self.reader)
            .query_row(&format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE id = ?"), [id], workspace_from_row)
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no workspace {id}")))
    }

    pub fn list_workspaces(&self, include_archived: bool) -> Result<Vec<Workspace>, ApiError> {
        let conn = lock(&self.reader);
        let filter = if include_archived { "" } else { "WHERE status != 'archived'" };
        let mut statement = conn
            .prepare_cached(&format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces {filter} ORDER BY created_at_ms"))
            .map_err(internal)?;
        let rows = statement.query_map([], workspace_from_row).map_err(internal)?;
        rows.collect::<rusqlite::Result<_>>().map_err(internal)
    }

    pub fn names_in_repo(&self, repo_id: &str) -> Result<Vec<String>, ApiError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT name FROM workspaces WHERE repo_id = ?").map_err(internal)?;
        let rows = statement.query_map([repo_id], |row| row.get(0)).map_err(internal)?;
        rows.collect::<rusqlite::Result<_>>().map_err(internal)
    }

    /// A workspace left half-made by a crash has no usable worktree or sessions.
    pub fn take_unfinished_workspaces(&self) -> Result<Vec<Workspace>, ApiError> {
        let conn = lock(&self.writer);
        let mut statement = conn
            .prepare_cached(&format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE status = ?"))
            .map_err(internal)?;
        let creating = text_of(&WorkspaceStatus::Creating);
        let rows = statement.query_map([&creating], workspace_from_row).map_err(internal)?;
        let workspaces = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(internal)?;
        conn.execute("DELETE FROM workspaces WHERE status = ?", [&creating]).map_err(internal)?;
        Ok(workspaces)
    }

    // Sessions ------------------------------------------------------------------------------------

    pub fn insert_session(&self, stored: &StoredSession) -> Result<(), ApiError> {
        let session = &stored.session;
        lock(&self.writer)
            .execute(
                &format!("INSERT INTO sessions ({SESSION_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"),
                params![
                    session.id,
                    session.workspace_id,
                    text_of(&session.agent),
                    session.model,
                    session.permission_mode,
                    text_of(&session.status),
                    session.last_seq as i64,
                    session.created_at_ms,
                    session.updated_at_ms,
                    stored.agent_session_id,
                    stored.agent_opened,
                    session.title,
                    session.closed_at_ms,
                ],
            )
            .map_err(internal)?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<StoredSession, ApiError> {
        lock(&self.reader)
            .query_row(&format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?"), [id], session_from_row)
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no session {id}")))
    }

    pub fn list_sessions(&self, workspace_id: &str) -> Result<Vec<Session>, ApiError> {
        let conn = lock(&self.reader);
        let mut statement = conn
            .prepare_cached(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions WHERE workspace_id = ? ORDER BY created_at_ms"
            ))
            .map_err(internal)?;
        let rows = statement.query_map([workspace_id], session_from_row).map_err(internal)?;
        let sessions = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(internal)?;
        Ok(sessions.into_iter().map(|s| s.session).collect())
    }

    /// `None` leaves that setting as it is.
    pub fn set_session_options(
        &self,
        id: &str,
        model: Option<&str>,
        permission_mode: Option<&str>,
    ) -> Result<(), ApiError> {
        lock(&self.writer)
            .execute(
                "UPDATE sessions SET model = COALESCE(?, model), permission_mode = COALESCE(?, permission_mode) WHERE id = ?",
                params![model, permission_mode, id],
            )
            .map_err(internal)?;
        Ok(())
    }

    pub fn set_session_title(&self, id: &str, title: &str) -> Result<(), ApiError> {
        lock(&self.writer)
            .execute("UPDATE sessions SET title = ? WHERE id = ?", params![title, id])
            .map_err(internal)?;
        Ok(())
    }

    pub fn set_session_closed(&self, id: &str, closed_at_ms: Option<i64>) -> Result<(), ApiError> {
        lock(&self.writer)
            .execute("UPDATE sessions SET closed_at_ms = ? WHERE id = ?", params![closed_at_ms, id])
            .map_err(internal)?;
        Ok(())
    }

    pub fn set_agent_opened(&self, id: &str) -> Result<(), ApiError> {
        lock(&self.writer).execute("UPDATE sessions SET agent_opened = 1 WHERE id = ?", [id]).map_err(internal)?;
        Ok(())
    }

    /// Sessions that were mid-turn when the hub last stopped.
    pub fn active_session_ids(&self) -> Result<Vec<String>, ApiError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT id FROM sessions WHERE status IN (?, ?)").map_err(internal)?;
        let rows = statement
            .query_map([text_of(&SessionStatus::Running), text_of(&SessionStatus::WaitingApproval)], |row| row.get(0))
            .map_err(internal)?;
        rows.collect::<rusqlite::Result<_>>().map_err(internal)
    }

    // Comments ------------------------------------------------------------------------------------

    pub fn add_comment(&self, comment: &DiffComment) -> Result<(), ApiError> {
        lock(&self.writer)
            .execute(
                "INSERT INTO diff_comments (id, workspace_id, path, side, line, text, created_at_ms)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    comment.id,
                    comment.workspace_id,
                    comment.path,
                    text_of(&comment.side),
                    comment.line,
                    comment.text,
                    comment.created_at_ms,
                ],
            )
            .map_err(internal)?;
        Ok(())
    }

    pub fn list_comments(&self, workspace_id: &str) -> Result<Vec<DiffComment>, ApiError> {
        let conn = lock(&self.reader);
        let mut statement = conn
            .prepare_cached(
                "SELECT id, workspace_id, path, side, line, text, created_at_ms FROM diff_comments
                 WHERE workspace_id = ? ORDER BY created_at_ms, id",
            )
            .map_err(internal)?;
        let rows = statement
            .query_map([workspace_id], |row| {
                Ok(DiffComment {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    path: row.get(2)?,
                    side: from_text(row.get(3)?)?,
                    line: row.get(4)?,
                    text: row.get(5)?,
                    created_at_ms: row.get(6)?,
                })
            })
            .map_err(internal)?;
        rows.collect::<rusqlite::Result<_>>().map_err(internal)
    }

    pub fn delete_comment(&self, id: &str) -> Result<(), ApiError> {
        lock(&self.writer).execute("DELETE FROM diff_comments WHERE id = ?", [id]).map_err(internal)?;
        Ok(())
    }

    // Events --------------------------------------------------------------------------------------

    /// `new_status` commits with the event, so nobody can see one without the other. A client that
    /// reads status after an event gets at least that event's status.
    pub fn append(
        &self,
        session_id: &str,
        body: &EventBody,
        raw: Option<&str>,
        client_msg_id: Option<&str>,
        new_status: Option<SessionStatus>,
    ) -> Result<Appended, ApiError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(internal)?;
        if let Some(client_msg_id) = client_msg_id {
            let existing = tx
                .query_row(
                    "SELECT seq, ts_ms, body FROM events WHERE session_id = ? AND client_msg_id = ?",
                    params![session_id, client_msg_id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
                )
                .optional()
                .map_err(internal)?;
            if let Some((seq, ts_ms, body)) = existing {
                let body = serde_json::from_str(&body).map_err(internal)?;
                let event =
                    SessionEvent { session_id: session_id.to_string(), seq: seq as Seq, ts_ms, body, raw: None };
                return Ok(Appended { event, duplicate: true });
            }
        }
        let last_seq: Option<i64> = tx
            .query_row("SELECT last_seq FROM sessions WHERE id = ?", [session_id], |row| row.get(0))
            .optional()
            .map_err(internal)?;
        let last_seq =
            last_seq.ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no session {session_id}")))?;
        let seq = last_seq + 1;
        let ts_ms = now_ms();
        let encoded = serde_json::to_string(body).map_err(internal)?;
        tx.execute(
            "INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, body, raw) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![session_id, seq, ts_ms, body.kind(), client_msg_id, encoded, raw],
        )
        .map_err(internal)?;
        tx.execute("UPDATE sessions SET last_seq = ?, updated_at_ms = ? WHERE id = ?", params![seq, ts_ms, session_id])
            .map_err(internal)?;
        if let Some(status) = new_status {
            tx.execute("UPDATE sessions SET status = ? WHERE id = ?", params![text_of(&status), session_id])
                .map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
        let event =
            SessionEvent { session_id: session_id.to_string(), seq: seq as Seq, ts_ms, body: body.clone(), raw: None };
        Ok(Appended { event, duplicate: false })
    }

    /// Always returns at least one event if any exist at or after `from_seq`, however large.
    pub fn read_events(
        &self,
        session_id: &str,
        from_seq: Seq,
        limit: u32,
        max_bytes: u64,
        include_raw: bool,
    ) -> Result<Vec<SessionEvent>, ApiError> {
        let conn = lock(&self.reader);
        let raw_column = if include_raw { "raw" } else { "NULL" };
        let mut statement = conn
            .prepare_cached(&format!(
                "SELECT seq, ts_ms, body, {raw_column} FROM events WHERE session_id = ? AND seq >= ? ORDER BY seq LIMIT ?"
            ))
            .map_err(internal)?;
        let mut rows = statement.query(params![session_id, from_seq as i64, limit]).map_err(internal)?;
        let mut events = Vec::new();
        let mut bytes = 0u64;
        while let Some(row) = rows.next().map_err(internal)? {
            let body: String = row.get(2).map_err(internal)?;
            let raw: Option<String> = row.get(3).map_err(internal)?;
            bytes += (body.len() + raw.as_ref().map_or(0, String::len)) as u64;
            events.push(SessionEvent {
                session_id: session_id.to_string(),
                seq: row.get::<_, i64>(0).map_err(internal)? as Seq,
                ts_ms: row.get(1).map_err(internal)?,
                body: serde_json::from_str(&body).map_err(internal)?,
                raw,
            });
            if bytes >= max_bytes {
                break;
            }
        }
        Ok(events)
    }

    /// Approvals requested and never resolved.
    pub fn open_approvals(&self, session_id: &str) -> Result<Vec<String>, ApiError> {
        let conn = lock(&self.reader);
        let mut statement = conn
            .prepare_cached(
                "SELECT body FROM events WHERE session_id = ? AND kind IN ('approval_requested', 'approval_resolved') ORDER BY seq",
            )
            .map_err(internal)?;
        let rows = statement.query_map([session_id], |row| row.get::<_, String>(0)).map_err(internal)?;
        let mut open = Vec::new();
        for body in rows {
            let body: EventBody = serde_json::from_str(&body.map_err(internal)?).map_err(internal)?;
            match body {
                EventBody::ApprovalRequested { approval_id, .. } => open.push(approval_id),
                EventBody::ApprovalResolved { approval_id, .. } => open.retain(|id| *id != approval_id),
                _ => {}
            }
        }
        Ok(open)
    }
}

fn migrate(conn: &mut Connection) -> anyhow::Result<()> {
    let version: usize = conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))? as usize;
    if version > MIGRATIONS.len() {
        anyhow::bail!(
            "the database was written by a newer version ({version} migrations, this build has {})",
            MIGRATIONS.len()
        );
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

/// The event that closes an unanswered approval when nobody can answer it any more.
pub fn cancelled_approval(approval_id: String) -> EventBody {
    EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::event::NoticeLevel;

    fn store_with_session(dir: &Path) -> Store {
        let store = Store::open(&dir.join("hub.db")).unwrap();
        let now = now_ms();
        store
            .add_repo(&Repo { id: "r".into(), name: "repo".into(), path: "/repo".into(), created_at_ms: now })
            .unwrap();
        store
            .insert_workspace(&Workspace {
                id: "w".into(),
                repo_id: "r".into(),
                name: "alder".into(),
                branch: "alder".into(),
                base_ref: None,
                path: None,
                status: WorkspaceStatus::Ready,
                provider: "local".into(),
                instance_id: "w".into(),
                created_at_ms: now,
                archived_at_ms: None,
                setup: SetupStatus::NotStarted,
                port_base: 55_000,
            })
            .unwrap();
        store
            .insert_session(&StoredSession {
                session: Session {
                    id: "s".into(),
                    workspace_id: "w".into(),
                    agent: AgentKind::Claude,
                    model: None,
                    permission_mode: None,
                    title: None,
                    closed_at_ms: None,
                    status: SessionStatus::Idle,
                    last_seq: 0,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
                agent_session_id: "s".into(),
                agent_opened: false,
            })
            .unwrap();
        store
    }

    fn notice(text: &str) -> EventBody {
        EventBody::Notice { level: NoticeLevel::Info, message: text.into() }
    }

    #[test]
    fn sequence_numbers_start_at_one_with_no_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        for expected in 1..=5 {
            assert_eq!(store.append("s", &notice("x"), None, None, None).unwrap().event.seq, expected);
        }
        let events = store.read_events("s", 1, 100, u64::MAX, false).unwrap();
        assert_eq!(events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
        assert_eq!(store.get_session("s").unwrap().session.last_seq, 5);
    }

    #[test]
    fn a_status_change_commits_with_its_event() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        store.append("s", &notice("x"), None, None, Some(SessionStatus::Running)).unwrap();
        assert_eq!(store.get_session("s").unwrap().session.status, SessionStatus::Running);
        store.append("s", &notice("x"), None, None, None).unwrap();
        assert_eq!(store.get_session("s").unwrap().session.status, SessionStatus::Running);
    }

    #[test]
    fn a_repeated_client_msg_id_returns_the_original_and_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        let first = store.append("s", &notice("a"), None, Some("msg-1"), None).unwrap();
        assert!(!first.duplicate);
        store.append("s", &notice("b"), None, None, None).unwrap();
        let again = store.append("s", &notice("c"), None, Some("msg-1"), None).unwrap();
        assert!(again.duplicate);
        assert_eq!(again.event.seq, first.event.seq);
        assert_eq!(again.event.body, notice("a"));
        assert_eq!(store.get_session("s").unwrap().session.last_seq, 2);
    }

    #[test]
    fn reads_page_by_count_and_by_size_and_always_make_progress() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        for _ in 0..10 {
            store.append("s", &notice(&"x".repeat(1000)), None, None, None).unwrap();
        }
        assert_eq!(store.read_events("s", 1, 3, u64::MAX, false).unwrap().len(), 3);
        assert_eq!(store.read_events("s", 9, 100, u64::MAX, false).unwrap().len(), 2);
        assert_eq!(store.read_events("s", 11, 100, u64::MAX, false).unwrap().len(), 0);
        // Stops at the size limit, but a single event larger than the limit still comes back.
        assert_eq!(store.read_events("s", 1, 100, 2500, false).unwrap().len(), 3);
        assert_eq!(store.read_events("s", 1, 100, 1, false).unwrap().len(), 1);
    }

    #[test]
    fn raw_lines_are_kept_and_only_returned_when_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        store.append("s", &notice("x"), Some("{\"original\":true}"), None, None).unwrap();
        assert_eq!(store.read_events("s", 1, 1, u64::MAX, false).unwrap()[0].raw, None);
        assert_eq!(
            store.read_events("s", 1, 1, u64::MAX, true).unwrap()[0].raw.as_deref(),
            Some("{\"original\":true}")
        );
    }

    #[test]
    fn open_approvals_are_those_requested_and_not_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        let requested = |id: &str| EventBody::ApprovalRequested {
            approval_id: id.into(),
            tool_name: "Bash".into(),
            tool_use_id: None,
            input: Value::Null,
            description: None,
            suggestions: Value::Null,
        };
        store.append("s", &requested("a"), None, None, None).unwrap();
        store.append("s", &requested("b"), None, None, None).unwrap();
        store.append("s", &cancelled_approval("a".into()), None, None, None).unwrap();
        assert_eq!(store.open_approvals("s").unwrap(), vec!["b".to_string()]);
    }

    #[test]
    fn data_survives_reopening_and_migrations_do_not_rerun() {
        let dir = tempfile::tempdir().unwrap();
        store_with_session(dir.path()).append("s", &notice("kept"), None, None, None).unwrap();
        let store = Store::open(&dir.path().join("hub.db")).unwrap();
        assert_eq!(store.read_events("s", 1, 10, u64::MAX, false).unwrap()[0].body, notice("kept"));
    }

    #[test]
    fn a_database_from_a_newer_version_is_refused_rather_than_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hub.db");
        drop(Store::open(&path).unwrap());
        Connection::open(&path).unwrap().pragma_update(None, "user_version", 999).unwrap();
        assert!(Store::open(&path).is_err());
    }

    #[test]
    fn comments_belong_to_a_workspace_and_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        let comment = DiffComment {
            id: "c1".into(),
            workspace_id: "w".into(),
            path: "src/a.rs".into(),
            side: protocol::model::DiffSide::New,
            line: 12,
            text: "Rename this".into(),
            created_at_ms: 1,
        };
        store.add_comment(&comment).unwrap();
        assert_eq!(store.list_comments("w").unwrap(), vec![comment]);
        assert_eq!(store.list_comments("other").unwrap(), vec![]);
        store.delete_comment("c1").unwrap();
        assert_eq!(store.list_comments("w").unwrap(), vec![]);
    }

    #[test]
    fn each_workspace_gets_its_own_ports() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        assert_eq!(store.next_port_base().unwrap(), 55_010);
        let mut next = store.get_workspace("w").unwrap();
        next.id = "w2".into();
        next.name = "birch".into();
        next.port_base = 55_010;
        store.insert_workspace(&next).unwrap();
        assert_eq!(store.next_port_base().unwrap(), 55_020);
    }

    #[test]
    fn a_setup_cut_off_by_a_restart_counts_as_failed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        store.set_workspace_setup("w", SetupStatus::Running).unwrap();
        store.fail_interrupted_setups().unwrap();
        assert_eq!(store.get_workspace("w").unwrap().setup, SetupStatus::Failed { code: None });
        store.set_workspace_setup("w", SetupStatus::Succeeded).unwrap();
        store.fail_interrupted_setups().unwrap();
        assert_eq!(store.get_workspace("w").unwrap().setup, SetupStatus::Succeeded);
    }

    #[test]
    fn adding_a_repo_twice_returns_the_same_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        let again = store
            .add_repo(&Repo { id: "other".into(), name: "repo".into(), path: "/repo".into(), created_at_ms: 0 })
            .unwrap();
        assert_eq!(again.id, "r");
    }

    #[test]
    fn workspace_names_are_unique_per_repo() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());
        let mut duplicate = store.get_workspace("w").unwrap();
        duplicate.id = "w2".into();
        assert!(!store.insert_workspace(&duplicate).unwrap());
    }
}
