use std::collections::HashSet;

use protocol::model::{Repo, RepoId, RepoOrigin, RepoUrl};
use protocol::worker::PushRoute;
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params};

use super::super::{Store, StoreError, lock, rev_of};

/// The columns every read of a repository's record takes, in this order, before its revision, as [`repo_from_row`]
/// reads them.
pub(in super::super) const REPO_COLUMNS: &str = "id, name, path, url, created_at_ms";

/// A row's record: its checkout, with the address a worker read from it, where one has. Every row has a checkout,
/// since a repository is added by one.
pub(in super::super) fn repo_from_row(row: &Row) -> rusqlite::Result<Repo> {
    let path = row.get(2)?;
    let origin = match row.get::<_, Option<String>>(3)? {
        Some(url) => RepoOrigin::Linked { path, url: url_at(3, url)? },
        None => RepoOrigin::Checkout { path },
    };
    Ok(Repo {
        id: row.get::<_, String>(0)?.into(),
        name: row.get(1)?,
        origin,
        created_at_ms: row.get(4)?,
        rev: rev_of(row, 5)?,
    })
}

/// The address in `column`, which is refused as a row that can't be read if it holds a credential, though only a
/// database changed by hand could hold one, since none is ever written.
fn url_at(column: usize, url: String) -> rusqlite::Result<RepoUrl> {
    RepoUrl::try_from(url).map_err(|e| rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(e)))
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
        conn.query_row(&format!("SELECT {REPO_COLUMNS}, rev FROM repos WHERE path = ?"), [checkout], repo_from_row)
            .map_err(StoreError::from)
    }

    /// Every repository, as [`Repo::by_name`] orders them.
    pub fn list_repos(&self) -> Result<Vec<Repo>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!("SELECT {REPO_COLUMNS}, rev FROM repos ORDER BY name, id"))?;
        let rows = statement.query_map([], repo_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    pub fn get_repo(&self, id: &RepoId) -> Result<Repo, StoreError> {
        lock(&self.reader)
            .query_row(&format!("SELECT {REPO_COLUMNS}, rev FROM repos WHERE id = ?"), [id.as_str()], repo_from_row)
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "repository", id: id.to_string() })
    }

    /// Each repository whose checkout's address is to be read, by name.
    pub fn repos_to_read(&self) -> Result<Vec<RepoToRead>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached("SELECT id, name, path FROM repos WHERE url_to_read = 1 ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(RepoToRead { id: row.get::<_, String>(0)?.into(), name: row.get(1)?, checkout: row.get(2)? })
        })?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// Each repository whose checkout's pushes go round the git credential, as the worker that last read its address
    /// said.
    pub fn repos_pushing_around(&self) -> Result<HashSet<RepoId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT id FROM repos WHERE push_around = 1")?;
        let rows = statement.query_map([], |row| Ok(row.get::<_, String>(0)?.into()))?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// Keeps `url` as the address of the repository `id`, as a worker read it from `checkout`, with where its pushes
    /// go, `push`, if its address is to be read and that's its checkout, and returns the record if that changed it,
    /// for the feed to tell of, which only the address does. Either way its address isn't to be read once that's what
    /// it is, so a second answer to the same ask changes nothing.
    pub fn read_repo_url(
        &self,
        id: &RepoId,
        checkout: &str,
        url: Option<&RepoUrl>,
        push: Option<PushRoute>,
    ) -> Result<Option<Repo>, StoreError> {
        let around = match push {
            Some(PushRoute::Around) => Some(true),
            Some(PushRoute::Credential) => Some(false),
            None | Some(PushRoute::Unknown) => None,
        };
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let was_to_read = tx.execute(
            "UPDATE repos SET url_to_read = 0, push_around = ? WHERE id = ? AND path = ? AND url_to_read = 1",
            params![around, id.as_str(), checkout],
        )?;
        let url = url.map(RepoUrl::as_str);
        let changed = was_to_read > 0
            && tx.execute("UPDATE repos SET url = ?2 WHERE id = ?1 AND url IS NOT ?2", params![id.as_str(), url])? > 0;
        let repo = if changed {
            let select = format!("SELECT {REPO_COLUMNS}, rev FROM repos WHERE id = ?");
            Some(tx.query_row(&select, [id.as_str()], repo_from_row)?)
        } else {
            None
        };
        tx.commit()?;
        Ok(repo)
    }
}
