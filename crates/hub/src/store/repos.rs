//! Repositories.

use protocol::model::Repo;
use rusqlite::{OptionalExtension, Row, params};

use super::{Store, StoreError, lock};

fn repo_from_row(row: &Row) -> rusqlite::Result<Repo> {
    Ok(Repo { id: row.get(0)?, name: row.get(1)?, path: row.get(2)?, created_at_ms: row.get(3)? })
}

impl Store {
    pub fn add_repo(&self, repo: &Repo) -> Result<Repo, StoreError> {
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO repos (id, name, path, created_at_ms) VALUES (?, ?, ?, ?) ON CONFLICT (path) DO NOTHING",
            params![repo.id, repo.name, repo.path, repo.created_at_ms],
        )?;
        // Adding a repository twice returns the first record.
        conn.query_row("SELECT id, name, path, created_at_ms FROM repos WHERE path = ?", [&repo.path], repo_from_row)
            .map_err(StoreError::from)
    }

    pub fn list_repos(&self) -> Result<Vec<Repo>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT id, name, path, created_at_ms FROM repos ORDER BY name")?;
        let rows = statement.query_map([], repo_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    pub fn get_repo(&self, id: &str) -> Result<Repo, StoreError> {
        lock(&self.reader)
            .query_row("SELECT id, name, path, created_at_ms FROM repos WHERE id = ?", [id], repo_from_row)
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "repository", id: id.to_string() })
    }
}
