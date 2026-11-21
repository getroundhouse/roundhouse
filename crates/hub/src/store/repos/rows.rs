use protocol::model::{Repo, RepoId, RepoOrigin, RepoUrl};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params};

use super::super::{At, ByColumn, Column, Columns, Store, StoreError, columns, lock, rev_of};

columns! {
    /// The columns every read of a repository's record takes, as [`repo_from_row`] reads them.
    pub(in super::super) struct RepoColumns { id, name, path, url, created_at_ms, rev }
}

/// A row's record: its checkout, with the address a worker read from it, where one has. Every row has a checkout,
/// since a repository is added by one.
pub(in super::super) fn repo_from_row(row: &Row) -> rusqlite::Result<Repo> {
    let c = RepoColumns::at(0);
    let path = row.at(c.path)?;
    let origin = match row.at::<Option<String>>(c.url)? {
        Some(url) => RepoOrigin::Linked { path, url: url_at(c.url, url)? },
        None => RepoOrigin::Checkout { path },
    };
    Ok(Repo {
        id: row.at::<String>(c.id)?.into(),
        name: row.at(c.name)?,
        origin,
        created_at_ms: row.at(c.created_at_ms)?,
        rev: rev_of(row, c.rev)?,
    })
}

/// The address in `column`, which is refused as a row that can't be read if it holds a credential, though only a
/// database changed by hand could hold one, since none is ever written.
fn url_at(column: Column, url: String) -> rusqlite::Result<RepoUrl> {
    RepoUrl::try_from(url).map_err(|e| column.unreadable(Type::Text, Box::new(e)))
}

/// A repository whose checkout's address is to be read, as [`Store::repos_to_read`] lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoToRead {
    pub id: RepoId,
    pub name: String,
    pub checkout: String,
}

impl Store {
    /// Adds the repository whose checkout on the hub's machine is `checkout`, with its address to be read, and returns
    /// its record. Adding one whose checkout is there already returns the first record, whose address is then to be
    /// read again, which doesn't change the record.
    pub fn add_repo(&self, id: &RepoId, name: &str, checkout: &str, created_at_ms: i64) -> Result<Repo, StoreError> {
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO repos (id, name, path, created_at_ms) VALUES (?, ?, ?, ?)
                 ON CONFLICT (path) DO UPDATE SET url_to_read = 1",
            params![id.as_str(), name, checkout, created_at_ms],
        )?;
        conn.query_row(&format!("SELECT {} FROM repos WHERE path = ?", RepoColumns::LIST), [checkout], repo_from_row)
            .map_err(StoreError::from)
    }

    /// Every repository, as [`Repo::by_name`] orders them.
    pub fn list_repos(&self) -> Result<Vec<Repo>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached(&format!("SELECT {} FROM repos ORDER BY name, id", RepoColumns::LIST))?;
        let rows = statement.query_map([], repo_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    pub fn get_repo(&self, id: &RepoId) -> Result<Repo, StoreError> {
        lock(&self.reader)
            .query_row(&format!("SELECT {} FROM repos WHERE id = ?", RepoColumns::LIST), [id.as_str()], repo_from_row)
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "repository", id: id.to_string() })
    }

    /// Each repository whose checkout's address is to be read, by name.
    pub fn repos_to_read(&self) -> Result<Vec<RepoToRead>, StoreError> {
        let conn = lock(&self.reader);
        columns! {
            struct ToRead { id, name, path }
        }
        let c = ToRead::at(0);
        let mut statement =
            conn.prepare_cached(&format!("SELECT {} FROM repos WHERE url_to_read = 1 ORDER BY name", ToRead::LIST))?;
        let rows = statement.query_map([], |row| {
            Ok(RepoToRead { id: row.at::<String>(c.id)?.into(), name: row.at(c.name)?, checkout: row.at(c.path)? })
        })?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// Keeps `url` as the address of the repository `id`, as a worker read it from `checkout`, if its address is to be
    /// read and that's its checkout, and returns the record if that changed it, for the feed to tell of. Either way its
    /// address isn't to be read once that's what it is, so a second answer to the same ask changes nothing.
    pub fn read_repo_url(
        &self,
        id: &RepoId,
        checkout: &str,
        url: Option<&RepoUrl>,
    ) -> Result<Option<Repo>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let was_to_read = tx.execute(
            "UPDATE repos SET url_to_read = 0 WHERE id = ? AND path = ? AND url_to_read = 1",
            params![id.as_str(), checkout],
        )?;
        let url = url.map(RepoUrl::as_str);
        let changed = was_to_read > 0
            && tx.execute("UPDATE repos SET url = ?2 WHERE id = ?1 AND url IS NOT ?2", params![id.as_str(), url])? > 0;
        let repo = if changed {
            let select = format!("SELECT {} FROM repos WHERE id = ?", RepoColumns::LIST);
            Some(tx.query_row(&select, [id.as_str()], repo_from_row)?)
        } else {
            None
        };
        tx.commit()?;
        Ok(repo)
    }
}
