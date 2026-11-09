//! Titles, and what each way of setting one may replace, so that rule is written once for workspaces and chats.

use protocol::model::{Session, SessionId, Title, TitleSource, Workspace, WorkspaceId};
use rusqlite::{OptionalExtension, Row, params};

use super::sessions::{SESSION_COLUMNS, session_from_row};
use super::workspaces::{WORKSPACE_COLUMNS, workspace_from_row};
use super::{Store, StoreError, lock, text_of};

#[cfg(test)]
mod tests;

/// How a title is being set, which decides what it may replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retitle {
    /// From a prompt's first line, only where there's no title yet.
    FirstLine,
    /// Typed by someone, over any title.
    ByHand,
}

impl Retitle {
    fn source(self) -> TitleSource {
        match self {
            Retitle::FirstLine => TitleSource::FirstLine,
            Retitle::ByHand => TitleSource::ByHand,
        }
    }

    /// What has to be true of the record's title for this to replace it, as SQL.
    fn replaces(self) -> &'static str {
        match self {
            Retitle::FirstLine => "title IS NULL",
            Retitle::ByHand => "TRUE",
        }
    }
}

/// A record after asking to set its title, and whether that changed it.
pub struct Retitled<T> {
    pub record: T,
    pub changed: bool,
}

impl Store {
    pub fn retitle_workspace(
        &self,
        id: &WorkspaceId,
        title: &Title,
        how: Retitle,
    ) -> Result<Retitled<Workspace>, StoreError> {
        let table = Table { name: "workspaces", kind: "workspace", columns: WORKSPACE_COLUMNS };
        self.retitle(table, id.as_str(), title, how, workspace_from_row)
    }

    pub fn retitle_session(
        &self,
        id: &SessionId,
        title: &Title,
        how: Retitle,
    ) -> Result<Retitled<Session>, StoreError> {
        let table = Table { name: "sessions", kind: "session", columns: SESSION_COLUMNS };
        self.retitle(table, id.as_str(), title, how, |row| session_from_row(row).map(|stored| stored.session))
    }

    /// Sets the title where `how` allows, and reads the record back on the writer, so what comes back is what the
    /// update left.
    fn retitle<T>(
        &self,
        table: Table,
        id: &str,
        title: &Title,
        how: Retitle,
        from_row: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Retitled<T>, StoreError> {
        let Table { name, kind, columns } = table;
        let conn = lock(&self.writer);
        let update = format!("UPDATE {name} SET title = ?1, title_source = ?2 WHERE id = ?3 AND {}", how.replaces());
        let changed = conn.execute(&update, params![title.as_str(), text_of(&how.source()), id])? > 0;
        let record = conn
            .query_row(&format!("SELECT {columns}, rev FROM {name} WHERE id = ?"), [id], from_row)
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind, id: id.to_string() })?;
        Ok(Retitled { record, changed })
    }
}

/// Where a kind of record is kept.
#[derive(Clone, Copy)]
struct Table {
    name: &'static str,
    kind: &'static str,
    columns: &'static str,
}
