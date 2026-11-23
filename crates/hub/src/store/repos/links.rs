//! Each repository's link to its repository on a host, and why one whose address is on github.com isn't linked.

use std::collections::BTreeSet;

use protocol::model::{GithubRepositoryId, GithubRepositoryName, Repo, RepoId, RepoLink, RepoUrl, Unlinked};
use rusqlite::params;

use super::super::{ByColumn, Columns, OneValue, Store, StoreError, columns, json_at, lock};
use super::rows::{RepoColumns, repo_from_row};

/// A repository whose address is on github.com and that isn't linked, as [`Store::repos_to_link`] lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoToLink {
    pub id: RepoId,
    /// The address it's to be linked by, which names `repository`.
    pub url: RepoUrl,
    pub repository: GithubRepositoryName,
}

impl Store {
    /// Each repository whose address is on github.com and that isn't linked, by name.
    pub fn repos_to_link(&self) -> Result<Vec<RepoToLink>, StoreError> {
        let conn = lock(&self.reader);
        columns! {
            struct ToLink { id, url }
        }
        let c = ToLink::at(0);
        let select = format!("SELECT {} FROM repos WHERE link IS NULL AND url IS NOT NULL ORDER BY name", ToLink::LIST);
        let mut statement = conn.prepare_cached(&select)?;
        let rows = statement.query_map([], |row| Ok((row.at::<String>(c.id)?, row.at::<String>(c.url)?)))?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        // An address that can't be one is refused as its record is read, so it's left unlinked here.
        let to_link = rows.into_iter().filter_map(|(id, url)| {
            let url = RepoUrl::try_from(url).ok()?;
            Some(RepoToLink { id: id.into(), repository: url.on_github()?, url })
        });
        Ok(to_link.collect())
    }

    /// Links the repository `id` by `link`, if it isn't linked and its address is still `url`, the one it was found by,
    /// and returns its record if that changed it, for the feed to tell of. So a link found as the checkout is added
    /// again, and read from another address, is dropped.
    pub fn link_repo(&self, id: &RepoId, url: &RepoUrl, link: &RepoLink) -> Result<Option<Repo>, StoreError> {
        let conn = lock(&self.writer);
        let linked = conn.execute(
            "UPDATE repos SET link = ?3, unlinked = NULL WHERE id = ?1 AND url = ?2 AND link IS NULL",
            params![id.as_str(), url.as_str(), serde_json::to_string(link)?],
        )?;
        changed_repo(&conn, id, linked)
    }

    /// Keeps `why` the repository `id` isn't linked, if it isn't and its address is still `url`, and returns its record
    /// if that changed it, for the feed to tell of.
    pub fn repo_unlinked(&self, id: &RepoId, url: &RepoUrl, why: &Unlinked) -> Result<Option<Repo>, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(
            "UPDATE repos SET unlinked = ?3 WHERE id = ?1 AND url = ?2 AND link IS NULL AND unlinked IS NOT ?3",
            params![id.as_str(), url.as_str(), serde_json::to_string(why)?],
        )?;
        changed_repo(&conn, id, changed)
    }

    /// GitHub's ID for each repository on GitHub a repository is linked to, each once.
    pub fn github_links(&self) -> Result<BTreeSet<GithubRepositoryId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT link FROM repos WHERE link IS NOT NULL")?;
        let links = statement.query_map([], |row| json_at::<RepoLink>(row, OneValue))?;
        let mut ids = BTreeSet::new();
        for link in links {
            if let Some(RepoLink::Github { id, .. }) = link? {
                ids.insert(id);
            }
        }
        Ok(ids)
    }
}

impl Store {
    /// Adds the repository `name`, with no checkout on the hub's machine, linked by `link`, found by `url`, the address
    /// a person typed or GitHub's for one they picked, and returns its record. Adding one already linked the same way,
    /// to the same repository on GitHub, or by the same address on another host, with no checkout, returns the first
    /// record, unchanged.
    pub fn add_linked_repo(
        &self,
        id: &RepoId,
        name: &str,
        url: &RepoUrl,
        link: &RepoLink,
        created_at_ms: i64,
    ) -> Result<Repo, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let select = format!("SELECT {} FROM repos WHERE path IS NULL", RepoColumns::LIST);
        let held = tx.prepare(&select)?.query_map([], repo_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        if let Some(held) = held.into_iter().find(|held| held.link.link().is_some_and(|held| same(held, link))) {
            return Ok(held);
        }
        tx.execute(
            "INSERT INTO repos (id, name, path, url, url_to_read, link, created_at_ms) VALUES (?, ?, NULL, ?, 0, ?, ?)",
            params![id.as_str(), name, url.as_str(), serde_json::to_string(link)?, created_at_ms],
        )?;
        let select = format!("SELECT {} FROM repos WHERE id = ?", RepoColumns::LIST);
        let repo = tx.query_row(&select, [id.as_str()], repo_from_row)?;
        tx.commit()?;
        Ok(repo)
    }
}

/// Whether `held` and `link` link to the same repository, which for GitHub is its ID, however it was found.
fn same(held: &RepoLink, link: &RepoLink) -> bool {
    match (held, link) {
        (RepoLink::Github { id: held, .. }, RepoLink::Github { id, .. }) => held == id,
        (RepoLink::Plain { url: held }, RepoLink::Plain { url }) => held == url,
        (RepoLink::Github { .. } | RepoLink::Plain { .. } | RepoLink::Unknown, _) => false,
    }
}

/// The record of `id`, when `changed` rows say it changed.
fn changed_repo(conn: &rusqlite::Connection, id: &RepoId, changed: usize) -> Result<Option<Repo>, StoreError> {
    if changed == 0 {
        return Ok(None);
    }
    let select = format!("SELECT {} FROM repos WHERE id = ?", RepoColumns::LIST);
    Ok(Some(conn.query_row(&select, [id.as_str()], repo_from_row)?))
}
