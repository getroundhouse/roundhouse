use protocol::model::{GithubReach, Reach, RepoId};
use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::super::{Store, StoreError, json_at, lock, rev_of};

/// The columns every read of a reach's row takes, in this order, as [`reach_at`] reads them.
const COLUMNS: &str = "repo_id, reach, answered_at_ms, stale, rev FROM github_reach";

/// A row's record. A reach this build can't read, which only a newer build could have written, reads as
/// [`Reach::Unknown`], and is checked again as the hub starts.
fn reach_at(row: &Row<'_>) -> rusqlite::Result<GithubReach> {
    Ok(GithubReach {
        repo: row.get::<_, String>(0)?.into(),
        reach: json_at(row, 1)?.unwrap_or(Reach::Unknown),
        answered_at_ms: row.get(2)?,
        stale: row.get(3)?,
        rev: rev_of(row, 4)?,
    })
}

/// Every repository's reach, by repository, for a list of every record.
pub(in super::super) fn github_reach(conn: &Connection) -> rusqlite::Result<Vec<GithubReach>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} ORDER BY repo_id"))?;
    statement.query_map([], reach_at)?.collect()
}

impl Store {
    /// Records that a push from `repo`'s workspaces would meet `reach`, which GitHub answered at `answered_at_ms`, or
    /// which isn't GitHub's answer when that's `None`, as of this run, and returns the record, whose revision moves
    /// only when it's another record than the one kept.
    pub(crate) fn record_github_reach(
        &self,
        repo: &RepoId,
        reach: &Reach,
        answered_at_ms: Option<i64>,
    ) -> Result<GithubReach, StoreError> {
        let reach = serde_json::to_string(reach)?;
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO github_reach (repo_id, reach, answered_at_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT (repo_id) DO UPDATE SET reach = excluded.reach, answered_at_ms = excluded.answered_at_ms,
                     stale = 0
                 WHERE reach != excluded.reach OR answered_at_ms IS NOT excluded.answered_at_ms OR stale != 0",
            params![repo.as_str(), reach, answered_at_ms],
        )?;
        Ok(conn.query_row(&format!("SELECT {COLUMNS} WHERE repo_id = ?"), [repo.as_str()], reach_at)?)
    }

    /// `repo`'s reach, if it has one.
    pub(crate) fn github_reach_of(&self, repo: &RepoId) -> Result<Option<GithubReach>, StoreError> {
        let conn = lock(&self.reader);
        Ok(conn.query_row(&format!("SELECT {COLUMNS} WHERE repo_id = ?"), [repo.as_str()], reach_at).optional()?)
    }

    /// Every repository's reach, as the hub last recorded it, which it checks again as it starts.
    pub(crate) fn github_reaches(&self) -> Result<Vec<GithubReach>, StoreError> {
        Ok(github_reach(&lock(&self.reader))?)
    }

    /// Has every repository's reach say the last run left it, as the hub starts, before any client can read one, so a
    /// client tells what GitHub said then from what it says now, until each is checked again.
    pub(crate) fn github_reach_left_by_the_last_run(&self) -> Result<(), StoreError> {
        lock(&self.writer).execute("UPDATE github_reach SET stale = 1 WHERE stale = 0", [])?;
        Ok(())
    }
}
