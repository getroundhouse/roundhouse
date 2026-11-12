//! SQLite storage. Synchronous, and only called through `spawn_blocking`.
//!
//! All SQL lives in this module, so it's the one thing a Postgres-backed hub would replace.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, MutexGuard};
use protocol::event::SessionEvent;
use protocol::model::{AgentSessionId, OpenApproval, Rev, Seq, Session, ToolUseId};
use rusqlite::{Connection, Row, Transaction};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::warn;

mod accounts;
mod approvals;
mod attachments;
mod backup;
mod builds;
mod children;
mod comments;
mod drafts;
mod error;
mod events;
mod find;
mod hub_settings;
mod instances;
mod lines;
mod logins;
mod made;
mod new_workspace_draft;
#[cfg(test)]
mod older;
mod prompts;
mod queue;
mod reading;
mod records;
mod repos;
mod schema;
mod sent;
mod sessions;
mod subagents;
#[cfg(test)]
pub(crate) mod tests;
mod titles;
mod tools;
mod waits;
mod workspaces;

use schema::{migrate, open_connection};

pub use accounts::Launched;
pub use attachments::AddedBy;
pub use comments::Commented;
pub use error::{BackupError, StoreError};
pub use events::{Places, SessionChange, StatusChange, TurnEdge, cancelled_approval};
pub use instances::{InstanceStage, RecordedInstance};
pub(crate) use logins::{Connecting, IdFrom};
pub use made::{Made, MadeByRequest};
pub use queue::{Changed, Queued};
pub use reading::Reading;
pub use titles::{Retitle, Retitled};
pub use waits::Seen;
pub use workspaces::{BranchesSet, Ended, Move};

pub struct Store {
    /// One writer, which is what gives each session a gapless sequence.
    writer: Mutex<Connection>,
    /// A separate connection, so a large read doesn't delay an agent's events. WAL allows this.
    reader: Mutex<Connection>,
    /// Another, for finding text in a session, which reads all of it, so every client's reads go on meanwhile.
    finder: Mutex<Connection>,
}

/// A session record, plus the fields only the hub needs.
#[derive(Clone, Debug)]
pub struct StoredSession {
    pub session: Session,
    pub agent_session_id: AgentSessionId,
    /// Whether the agent has opened this conversation, which decides how it's launched next.
    pub agent_opened: bool,
}

pub struct Appended {
    pub event: SessionEvent,
    /// This `client_msg_id` was already recorded, and `event` is the original.
    pub duplicate: bool,
    /// The event is a prompt that was waiting in the queue, and recording it took it out.
    pub dequeued: bool,
    /// The person sent it, so it moved when they were last active, as `Session::user_active_at_ms` says.
    pub by_the_person: bool,
    /// The prompts the agent has yet to take, oldest first, if the event changed them.
    pub untaken: Option<Vec<Seq>>,
    /// The approvals the agent is waiting on, oldest first, if the event changed them.
    pub approvals: Option<Vec<OpenApproval>>,
    /// The subagents whose records the event changed.
    pub subagents: Vec<ToolUseId>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn lock(conn: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    conn.lock()
}

/// Enum to its `snake_case` serde name, for storing in a text column.
fn text_of<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        _ => unreachable!("only used with unit enums"),
    }
}

fn rev_of(row: &Row<'_>, column: usize) -> rusqlite::Result<Rev> {
    Ok(Rev::new(row.get::<_, i64>(column)?.cast_unsigned()))
}

impl Store {
    /// Runs `write` in a transaction whose commit is on disk before this returns, for what a power cut mustn't take
    /// back, such as a login the hub has said it holds. Every other commit can be lost to one, as `open_connection`
    /// says. `fullfsync` too, since without it SQLite's sync on a Mac can leave the commit in the drive's cache, as
    /// `backup` says of its copies, and `open_connection`'s `checkpoint_fullfsync` keeps it through the checkpoints
    /// after. SQLite won't change `synchronous` inside a transaction, so both are set around it, on the writer, which
    /// is held throughout so no other write is made with them.
    fn lasting<T>(&self, write: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<T>) -> Result<T, StoreError> {
        let mut conn = lock(&self.writer);
        let set =
            conn.pragma_update(None, "synchronous", "FULL").and_then(|()| conn.pragma_update(None, "fullfsync", true));
        let written = set.and_then(|()| {
            let tx = conn.transaction()?;
            let value = write(&tx)?;
            tx.commit()?;
            Ok(value)
        });
        // Put back on every path, once either may have been set. Left as it is, the writer would only sync more than
        // it needs to, so what was written still counts.
        let fullfsync = conn.pragma_update(None, "fullfsync", false);
        if let Err(e) = fullfsync.and(conn.pragma_update(None, "synchronous", "NORMAL")) {
            warn!("couldn't have the database sync as it usually does after a write that had to last: {e}");
        }
        Ok(written?)
    }

    /// Deletes the row with this ID from `table`. Returns the revision of the removal, or `None` if there
    /// was nothing to remove.
    fn delete(&self, table: &str, id: &str) -> Result<Option<Rev>, StoreError> {
        let conn = lock(&self.writer);
        let deleted = conn.execute(&format!("DELETE FROM {table} WHERE id = ?"), [id])?;
        Ok(if deleted == 0 { None } else { Some(current_rev(&conn)?) })
    }
}

/// The newest revision, which on the writer is the one its last change made.
fn current_rev(conn: &Connection) -> rusqlite::Result<Rev> {
    Ok(Rev::new(
        conn.query_row("SELECT value FROM counters WHERE name = 'rev'", [], |row| row.get::<_, i64>(0))?
            .cast_unsigned(),
    ))
}

fn from_text<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_value(Value::String(text))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

/// What a column holds as JSON, which only a hub writes, or `None` when it holds nothing, or what this build can't
/// read, such as one a build with another idea of it wrote. Each caller says what one it can't read means there.
fn json_at<T: DeserializeOwned>(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<T>> {
    Ok(row.get::<_, Option<String>>(column)?.and_then(|json| serde_json::from_str(&json).ok()))
}

impl Store {
    /// Opens the database at `path`, and migrates it if an older build wrote it, after saving a copy in
    /// `backups_dir`.
    pub fn open(path: &Path, backups_dir: &Path) -> Result<Self, StoreError> {
        let mut writer = open_connection(path)?;
        migrate(&mut writer, backups_dir)?;
        let reader = open_connection(path)?;
        let finder = open_connection(path)?;
        Ok(Self { writer: Mutex::new(writer), reader: Mutex::new(reader), finder: Mutex::new(finder) })
    }

    // Repos ---------------------------------------------------------------------------------------

    // Workspaces ----------------------------------------------------------------------------------

    // Sessions ------------------------------------------------------------------------------------

    // Comments ------------------------------------------------------------------------------------

    // Events --------------------------------------------------------------------------------------
}
