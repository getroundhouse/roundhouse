use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{GithubApp, GithubAppId, Rev};
use rusqlite::{Connection, OptionalExtension as _, Row, params};
use std::collections::HashSet;

use super::super::{ByColumn, Columns, OneValue, Store, StoreError, columns, current_rev, lock, rev_of};
use crate::login_files::LoginFile;

columns! {
    /// The columns every read of an app's row takes, as [`app_at`] reads them.
    struct AppColumns { id, rev, key_file }
}

/// An app of your own GitHub has just made, to keep: what Roundhouse keeps of it, while what GitHub says of it, its
/// slug, name and owner, the hub keeps as an observation, as `crate::hub::observations::GithubAppSaid` says.
#[derive(Clone, Copy)]
pub(crate) struct GithubAppMaking<'a> {
    pub(crate) id: GithubAppId,
    /// Where its key is.
    pub(crate) key: &'a LoginFile,
    /// What GitHub made it from, which it takes only once.
    pub(crate) made_from: &'a GithubManifestCode,
}

/// A row's record, and the file of its key, `None` for a row that names a file the hub doesn't make. What GitHub says
/// of the app isn't kept here, since the hub only observes it.
fn app_at(row: &Row<'_>) -> rusqlite::Result<(GithubApp, Option<LoginFile>)> {
    let c = AppColumns::at(0);
    let id = GithubAppId::new(row.at(c.id)?);
    let app = GithubApp { id, rev: rev_of(row, c.rev)? };
    Ok((app, LoginFile::parse(&row.at::<String>(c.key_file)?)))
}
/// Every app, oldest first, for a list of every record.
pub(in super::super) fn github_apps(conn: &Connection) -> rusqlite::Result<Vec<GithubApp>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {} FROM github_apps ORDER BY rowid", AppColumns::LIST))?;
    statement.query_map([], |row| app_at(row).map(|(app, _)| app))?.collect()
}

fn row_of(conn: &Connection, id: GithubAppId) -> rusqlite::Result<Option<(GithubApp, Option<LoginFile>)>> {
    conn.query_row(&format!("SELECT {} FROM github_apps WHERE id = ?", AppColumns::LIST), [id.get()], app_at).optional()
}

impl Store {
    pub(crate) fn github_app(&self, id: GithubAppId) -> Result<Option<GithubApp>, StoreError> {
        Ok(row_of(&lock(&self.reader), id)?.map(|(app, _)| app))
    }

    /// The app GitHub made from `code`, if the hub keeps one.
    pub(crate) fn github_app_made_from(&self, code: &GithubManifestCode) -> Result<Option<GithubApp>, StoreError> {
        let conn = lock(&self.reader);
        let found = conn.query_row(
            &format!("SELECT {} FROM github_apps WHERE made_from = ?", AppColumns::LIST),
            [code.as_str()],
            app_at,
        );
        Ok(found.optional()?.map(|(app, _)| app))
    }

    /// The app `id` and the file of its key, `None` for a file the hub doesn't make. Refused when there's no such app.
    pub(crate) fn github_app_key(&self, id: GithubAppId) -> Result<(GithubApp, Option<LoginFile>), StoreError> {
        let found = row_of(&lock(&self.reader), id)?;
        found.ok_or_else(|| StoreError::NotFound { kind: "app of your own", id: id.to_string() })
    }

    /// The file of every app's key, by app, for the hub to make the files and the rows agree as it starts.
    pub(crate) fn github_app_keys(&self) -> Result<Vec<(GithubAppId, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!("SELECT {} FROM github_apps", AppColumns::LIST))?;
        let rows = statement.query_map([], |row| app_at(row).map(|(app, file)| (app.id, file)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keeps the record of the app `making`, whose key is in `making.key` already, and answers with it. Its commit is
    /// on disk before this returns, so a key the hub has said it holds isn't lost to a power cut.
    pub(crate) fn make_github_app(&self, making: &GithubAppMaking<'_>) -> Result<GithubApp, StoreError> {
        let GithubAppMaking { id, key, made_from } = *making;
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO github_apps (id, key_file, made_from) VALUES (?, ?, ?)",
                params![id.get(), key.as_str(), made_from.as_str()],
            )?;
            Ok(row_of(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?.0)
        })
    }

    /// Every app's ID.
    pub(crate) fn github_app_ids(&self) -> Result<HashSet<GithubAppId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare("SELECT id FROM github_apps")?;
        let ids = statement.query_map([], OneValue::read::<u64>)?;
        Ok(ids.map(|id| id.map(GithubAppId::new)).collect::<rusqlite::Result<_>>()?)
    }

    /// Removes the app `id`'s record, whose key the hub has let go of, and answers with the revision of its removal, or
    /// `None` when there was no such app.
    pub(crate) fn remove_github_app(&self, id: GithubAppId) -> Result<Option<Rev>, StoreError> {
        let conn = lock(&self.writer);
        if conn.execute("DELETE FROM github_apps WHERE id = ?", [id.get()])? == 0 {
            return Ok(None);
        }
        Ok(Some(current_rev(&conn)?))
    }
}
