//! Every record at once, for a client that's starting to follow the feed.

use protocol::model::Records;
use rusqlite::{Row, Transaction};

use super::accounts::accounts;
use super::comments::{COMMENT_COLUMNS, comment_from_row};
use super::hub_settings::hub_settings;
use super::new_workspace_draft::new_workspace_draft;
use super::repos::{REPO_COLUMNS, repo_from_row};
use super::sessions::{SESSION_COLUMNS, session_from_row};
use super::workspaces::{WORKSPACE_COLUMNS, workspace_from_row};
use super::{Store, StoreError, current_rev, lock};

impl Store {
    /// Read in one transaction, so it's everything as of one revision.
    pub fn list_records(&self) -> Result<Records, StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let sessions =
            rows(&tx, &format!("SELECT {SESSION_COLUMNS}, rev FROM sessions ORDER BY created_at_ms"), |row| {
                session_from_row(row).map(|stored| stored.session)
            })?;
        let records = Records {
            repos: rows(&tx, &format!("SELECT {REPO_COLUMNS}, rev FROM repos ORDER BY name"), repo_from_row)?,
            workspaces: rows(
                &tx,
                &format!("SELECT {WORKSPACE_COLUMNS}, rev FROM workspaces ORDER BY created_at_ms"),
                workspace_from_row,
            )?,
            sessions,
            comments: rows(
                &tx,
                &format!("SELECT {COMMENT_COLUMNS}, rev FROM diff_comments ORDER BY created_at_ms, id"),
                comment_from_row,
            )?,
            hub_settings: Some(hub_settings(&tx)?),
            accounts: accounts(&tx)?,
            new_workspace_draft: Some(new_workspace_draft(&tx)?),
            // Kept in memory rather than here, as `Terminal` says, and none is opened yet.
            terminals: Vec::new(),
            rev: current_rev(&tx)?,
        };
        tx.commit()?;
        Ok(records)
    }
}

fn rows<T>(
    tx: &Transaction<'_>,
    sql: &str,
    from_row: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    tx.prepare(sql)?.query_map([], from_row)?.collect()
}
