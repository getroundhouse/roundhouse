//! Titles, and what each way of setting one may replace, so that rule is written once for workspaces and chats.

use protocol::model::{Session, SessionId, Title, TitleSource, Workspace, WorkspaceId};
use rusqlite::{OptionalExtension, Row, params};

use super::sessions::{SESSION_COLUMNS, session_from_row};
use super::workspaces::{WORKSPACE_COLUMNS, workspace_from_row};
use super::{Store, StoreError, from_text, lock, text_of};

#[cfg(test)]
mod tests;

/// How a title is being set, which decides what it may replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retitle<'a> {
    /// From a prompt's first line, only where there's no title yet.
    FirstLine,
    /// From a model asked for one, only over a first line, so a title typed meanwhile stays.
    Generated,
    /// From a model, because someone asked for a new title: over the one that was there when they asked, `over`,
    /// however it was `set`, and only that one, so a title set meanwhile stays. That includes the same words typed by
    /// hand, which is someone keeping a generated title.
    Regenerated { over: Option<&'a Title>, set: TitleSource },
    /// Typed by someone, over any title.
    ByHand,
}

impl Retitle<'_> {
    fn source(self) -> TitleSource {
        match self {
            Retitle::FirstLine => TitleSource::FirstLine,
            Retitle::Generated | Retitle::Regenerated { .. } => TitleSource::Generated,
            Retitle::ByHand => TitleSource::ByHand,
        }
    }

    /// What has to be true of the record's title for this to replace it, as SQL. [`Retitle::Regenerated`] also needs
    /// the title to be the one it names, which [`Store::retitle`] checks.
    fn replaces(self) -> &'static str {
        match self {
            Retitle::FirstLine => "title IS NULL",
            Retitle::Generated => "title IS NOT NULL AND title_source = 'first_line'",
            Retitle::Regenerated { .. } | Retitle::ByHand => "TRUE",
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
        how: Retitle<'_>,
    ) -> Result<Retitled<Workspace>, StoreError> {
        let table = Table { name: "workspaces", kind: "workspace", columns: WORKSPACE_COLUMNS };
        self.retitle(table, id.as_str(), title, how, workspace_from_row)
    }

    pub fn retitle_session(
        &self,
        id: &SessionId,
        title: &Title,
        how: Retitle<'_>,
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
        how: Retitle<'_>,
        from_row: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Retitled<T>, StoreError> {
        let Table { name, kind, columns } = table;
        let conn = lock(&self.writer);
        // Read on the writer, which stays locked until the update is done, so no title is set in between. Compared
        // tidied, as a record reads its title, so one stored before titles were tidied still counts as itself.
        let still_there = match how {
            Retitle::Regenerated { over, set } => {
                let read = format!("SELECT title, title_source FROM {name} WHERE id = ?");
                let now = conn.query_row(&read, [id], |row| {
                    Ok((row.get::<_, Option<String>>(0)?, from_text::<TitleSource>(row.get(1)?)?))
                });
                // One that's missing is found missing below.
                now.optional()?.is_none_or(|(title, source)| {
                    title.as_deref().and_then(Title::tidied).as_ref() == over && source == set
                })
            }
            Retitle::FirstLine | Retitle::Generated | Retitle::ByHand => true,
        };
        let update = format!("UPDATE {name} SET title = ?1, title_source = ?2 WHERE id = ?3 AND {}", how.replaces());
        let changed = still_there && conn.execute(&update, params![title.as_str(), text_of(&how.source()), id])? > 0;
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
