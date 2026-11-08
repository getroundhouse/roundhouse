//! Records a client makes with a request ID, so the same request sent again finds what it made.

use protocol::model::{ClientRequestId, DiffComment, Workspace};
use rusqlite::{Connection, OptionalExtension, Row};

use super::comments::{COMMENT_COLUMNS, comment_from_row};
use super::sessions::{SESSION_COLUMNS, session_from_row};
use super::workspaces::{WORKSPACE_COLUMNS, workspace_from_row};
use super::{Store, StoreError, StoredSession, lock};

/// A record a request made: just now, or the first time the same request was sent.
#[derive(Debug, PartialEq)]
pub enum Made<T> {
    New(T),
    Earlier(T),
}

/// A kind of record a request can make.
pub trait MadeByRequest: Sized {
    const TABLE: &'static str;
    /// What a row holds, apart from its revision.
    const COLUMNS: &'static str;
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self>;
}

impl MadeByRequest for Workspace {
    const TABLE: &'static str = "workspaces";
    const COLUMNS: &'static str = WORKSPACE_COLUMNS;
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        workspace_from_row(row)
    }
}

impl MadeByRequest for StoredSession {
    const TABLE: &'static str = "sessions";
    const COLUMNS: &'static str = SESSION_COLUMNS;
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        session_from_row(row)
    }
}

impl MadeByRequest for DiffComment {
    const TABLE: &'static str = "diff_comments";
    const COLUMNS: &'static str = COMMENT_COLUMNS;
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        comment_from_row(row)
    }
}

impl Store {
    /// What a request with this ID made, if it made anything.
    pub fn made_by<T: MadeByRequest>(&self, client_request_id: &ClientRequestId) -> Result<Option<T>, StoreError> {
        Ok(made_by(&lock(&self.reader), Some(client_request_id))?)
    }
}

/// Each insert calls this on the writer before it inserts, so two of the same request can't both insert.
pub(super) fn made_by<T: MadeByRequest>(
    conn: &Connection,
    client_request_id: Option<&ClientRequestId>,
) -> rusqlite::Result<Option<T>> {
    let Some(id) = client_request_id else { return Ok(None) };
    let query = format!("SELECT {}, rev FROM {} WHERE client_request_id = ?", T::COLUMNS, T::TABLE);
    conn.query_row(&query, [id.as_str()], T::from_row).optional()
}

/// The row just inserted with this ID, as stored, with its revision.
pub(super) fn inserted<T: MadeByRequest>(conn: &Connection, id: &str) -> rusqlite::Result<Made<T>> {
    let query = format!("SELECT {}, rev FROM {} WHERE id = ?", T::COLUMNS, T::TABLE);
    Ok(Made::New(conn.query_row(&query, [id], T::from_row)?))
}
