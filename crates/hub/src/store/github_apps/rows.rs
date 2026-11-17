use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{GithubApp, GithubAppId, GithubAppSlug, GithubUser, GithubUserId, Rev};
use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::super::{Store, StoreError, current_rev, lock, rev_of};
use crate::login_files::LoginFile;

/// The columns every read of an app's row takes, in this order, as [`app_at`] reads them.
const COLUMNS: &str = "id, slug, name, owner_id, owner, bot_id, bot, rev, key_file FROM github_apps";

/// An app of your own GitHub has just made, to keep.
#[derive(Clone, Copy)]
pub(crate) struct GithubAppMaking<'a> {
    pub(crate) id: GithubAppId,
    pub(crate) slug: &'a GithubAppSlug,
    pub(crate) name: &'a str,
    pub(crate) owner: &'a GithubUser,
    /// Where its key is.
    pub(crate) key: &'a LoginFile,
    /// What GitHub made it from, which it takes only once.
    pub(crate) made_from: &'a GithubManifestCode,
}

/// A row's record, and the file of its key, `None` for a row that names a file the hub doesn't make.
fn app_at(row: &Row<'_>) -> rusqlite::Result<(GithubApp, Option<LoginFile>)> {
    let bot = match (row.get::<_, Option<u64>>(5)?, row.get::<_, Option<String>>(6)?) {
        (Some(id), Some(name)) => Some(GithubUser { id: GithubUserId::new(id), name: name.into() }),
        _ => None,
    };
    let app = GithubApp {
        id: GithubAppId::new(row.get(0)?),
        slug: row.get::<_, String>(1)?.into(),
        name: row.get(2)?,
        owner: GithubUser { id: GithubUserId::new(row.get(3)?), name: row.get::<_, String>(4)?.into() },
        bot,
        rev: rev_of(row, 7)?,
    };
    Ok((app, LoginFile::parse(&row.get::<_, String>(8)?)))
}

/// Every app, oldest first, for a list of every record.
pub(in super::super) fn github_apps(conn: &Connection) -> rusqlite::Result<Vec<GithubApp>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} ORDER BY rowid"))?;
    statement.query_map([], |row| app_at(row).map(|(app, _)| app))?.collect()
}

fn row_of(conn: &Connection, id: GithubAppId) -> rusqlite::Result<Option<(GithubApp, Option<LoginFile>)>> {
    conn.query_row(&format!("SELECT {COLUMNS} WHERE id = ?"), [id.get()], app_at).optional()
}

impl Store {
    pub(crate) fn github_app(&self, id: GithubAppId) -> Result<Option<GithubApp>, StoreError> {
        Ok(row_of(&lock(&self.reader), id)?.map(|(app, _)| app))
    }

    /// The app GitHub made from `code`, if the hub keeps one.
    pub(crate) fn github_app_made_from(&self, code: &GithubManifestCode) -> Result<Option<GithubApp>, StoreError> {
        let conn = lock(&self.reader);
        let found = conn.query_row(&format!("SELECT {COLUMNS} WHERE made_from = ?"), [code.as_str()], app_at);
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
        let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS}"))?;
        let rows = statement.query_map([], |row| app_at(row).map(|(app, file)| (app.id, file)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keeps the record of the app `making`, whose key is in `making.key` already, and answers with it. Its commit is
    /// on disk before this returns, so a key the hub has said it holds isn't lost to a power cut.
    pub(crate) fn make_github_app(&self, making: &GithubAppMaking<'_>) -> Result<GithubApp, StoreError> {
        let GithubAppMaking { id, slug, name, owner, key, made_from } = *making;
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO github_apps (id, slug, name, owner_id, owner, key_file, made_from)
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    id.get(),
                    slug.as_str(),
                    name,
                    owner.id.get(),
                    owner.name.as_str(),
                    key.as_str(),
                    made_from.as_str()
                ],
            )?;
            Ok(row_of(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?.0)
        })
    }

    /// Says the app `id`'s bot is `bot`, as GitHub said, and answers with its record when that changed it.
    pub(crate) fn set_github_app_bot(
        &self,
        id: GithubAppId,
        bot: &GithubUser,
    ) -> Result<Option<GithubApp>, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(
            "UPDATE github_apps SET bot_id = ?, bot = ? WHERE id = ? AND (bot_id IS NOT ? OR bot IS NOT ?)",
            params![bot.id.get(), bot.name.as_str(), id.get(), bot.id.get(), bot.name.as_str()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(row_of(&conn, id)?.map(|(app, _)| app))
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
