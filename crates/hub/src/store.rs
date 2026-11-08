//! SQLite storage. Synchronous, and only called through `spawn_blocking`.
//!
//! All SQL lives in this module, so it's the one thing a Postgres-backed hub would replace.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, MutexGuard};
use protocol::event::SessionEvent;
use protocol::model::{AgentSessionId, Rev, Session};
use rusqlite::{Connection, Row};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

mod comments;
mod error;
mod events;
mod records;
mod repos;
mod schema;
mod sessions;
#[cfg(test)]
mod tests;
mod workspaces;

use schema::{migrate, open_connection};

pub use error::StoreError;
pub use events::cancelled_approval;

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
    pub agent_session_id: AgentSessionId,
    /// Whether the agent has opened this conversation, which decides how it's launched next.
    pub agent_opened: bool,
}

pub struct Appended {
    pub event: SessionEvent,
    /// This `client_msg_id` was already recorded, and `event` is the original.
    pub duplicate: bool,
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

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut writer = open_connection(path)?;
        migrate(&mut writer)?;
        let reader = open_connection(path)?;
        Ok(Self { writer: Mutex::new(writer), reader: Mutex::new(reader) })
    }

    // Repos ---------------------------------------------------------------------------------------

    // Workspaces ----------------------------------------------------------------------------------

    // Sessions ------------------------------------------------------------------------------------

    // Comments ------------------------------------------------------------------------------------

    // Events --------------------------------------------------------------------------------------
}
