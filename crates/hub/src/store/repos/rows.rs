use protocol::model::{Linking, Repo, RepoId, RepoLink, RepoOrigin, RepoUrl, Unlinked};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params};

use super::super::{At, ByColumn, Column, Columns, Store, StoreError, columns, json_at, lock, rev_of};

columns! {
    /// The columns every read of a repository's record takes, as [`repo_from_row`] reads them.
    pub(in super::super) struct RepoColumns { id, name, path, url, link, unlinked, created_at_ms, rev }
}

/// A row's record: its checkout, with the address a worker read from it, where one has, or the address it was added
/// by, for one added by its link alone, and its link.
pub(in super::super) fn repo_from_row(row: &Row) -> rusqlite::Result<Repo> {
    let c = RepoColumns::at(0);
    let path = row.at::<Option<String>>(c.path)?;
    let url = row.at::<Option<String>>(c.url)?.map(|url| url_at(c.url, url)).transpose()?;
    let link = linking(url.as_ref(), json_at(row, c.link)?, json_at(row, c.unlinked)?);
    let origin = match (path, url) {
        (Some(path), Some(url)) => RepoOrigin::Linked { path, url },
        (Some(path), None) => RepoOrigin::Checkout { path },
        (None, Some(url)) => RepoOrigin::Remote { url },
        // Neither, which no build writes.
        (None, None) => RepoOrigin::Unknown,
    };
    Ok(Repo {
        id: row.at::<String>(c.id)?.into(),
        name: row.at(c.name)?,
        origin,
        link,
        created_at_ms: row.at(c.created_at_ms)?,
        rev: rev_of(row, c.rev)?,
    })
}

/// Whether a repository whose address is `url` is linked, by its `link` and why it isn't, as its row keeps them. One
/// whose address is on github.com and that has no link is linked once GitHub says which repository it is, and one
/// whose address is anywhere else is linked by it as it's read, so a row with neither is one changed by hand, which
/// has no address to link by.
fn linking(url: Option<&RepoUrl>, link: Option<RepoLink>, unlinked: Option<Unlinked>) -> Linking {
    if let Some(link) = link {
        return Linking::Linked { link };
    }
    match url.and_then(RepoUrl::on_github) {
        Some(repository) => Linking::NotLinked { repository, why: unlinked.unwrap_or(Unlinked::NotAskedYet) },
        None => Linking::NoAddress,
    }
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
    /// address isn't to be read once that's what it is, so a second answer to the same ask changes nothing. An address
    /// other than the one it had takes the link with it, as adding the checkout again links it to what `origin` says
    /// then: one that isn't on github.com is its link at once, and one on github.com is linked once GitHub says which
    /// repository it names.
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
        let link = match url {
            Some(url) if url.on_github().is_none() => {
                Some(serde_json::to_string(&RepoLink::Plain { url: url.clone() })?)
            }
            Some(_) | None => None,
        };
        let changed = was_to_read > 0
            && tx.execute(
                "UPDATE repos SET url = ?2, link = ?3, unlinked = NULL WHERE id = ?1 AND url IS NOT ?2",
                params![id.as_str(), url.map(RepoUrl::as_str), link],
            )? > 0;
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
