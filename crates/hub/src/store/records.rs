//! Every record at once, for a client that's starting to follow the feed, and the records of one session at once, for
//! one that's starting to follow the session.

use protocol::client::listed::SessionRecords;
use protocol::model::{Records, SessionId};
use rusqlite::{Row, Transaction};

use super::accounts::accounts;
use super::comments::{CommentColumns, comment_from_row};
use super::github_accounts::github_accounts;
use super::github_apps::github_apps;
use super::github_reach::github_reach;
use super::github_sign_ins::github_sign_ins;
use super::hub_settings::hub_settings;
use super::new_workspace_draft::new_workspace_draft;
use super::proposals::proposals_of;
use super::replies_heard::replies_heard_of;
use super::repos::{RepoColumns, repo_from_row};
use super::sessions::{SessionColumns, session_from_row};
use super::subagents::subagents_of;
use super::workspaces::{WorkspaceColumns, workspace_from_row};
use super::{Columns, Store, StoreError, current_rev, lock};

impl Store {
    /// Read in one transaction, so it's everything as of one revision. Repositories, workspaces, sessions and comments
    /// are each in the order `protocol` lists them in, as [`Repo::by_name`] and the `oldest_first` of each of the others
    /// say.
    ///
    /// [`Repo::by_name`]: protocol::model::Repo::by_name
    pub fn list_records(&self) -> Result<Records, StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let sessions =
            rows(&tx, &format!("SELECT {} FROM sessions ORDER BY created_at_ms, id", SessionColumns::LIST), |row| {
                session_from_row(row).map(|stored| stored.session)
            })?;
        let records = Records {
            repos: rows(&tx, &format!("SELECT {} FROM repos ORDER BY name, id", RepoColumns::LIST), repo_from_row)?,
            workspaces: rows(
                &tx,
                &format!("SELECT {} FROM workspaces ORDER BY created_at_ms, id", WorkspaceColumns::LIST),
                workspace_from_row,
            )?,
            sessions,
            comments: rows(
                &tx,
                &format!("SELECT {} FROM diff_comments ORDER BY created_at_ms, id", CommentColumns::LIST),
                comment_from_row,
            )?,
            hub_settings: Some(hub_settings(&tx)?),
            accounts: accounts(&tx)?,
            new_workspace_draft: Some(new_workspace_draft(&tx)?),
            github_accounts: github_accounts(&tx)?,
            github_sign_ins: github_sign_ins(&tx)?,
            // A hub from before Settings' Providers found these, as `FoundGithubLogin` says. This one finds none.
            found_github_logins: Vec::new(),
            github_apps: github_apps(&tx)?,
            github_reach: github_reach(&tx)?,
            // Kept in memory rather than here, as `Terminal`, `ModelList` and `HubUpdate` say, so the hub adds them.
            terminals: Vec::new(),
            model_lists: Vec::new(),
            hub_update: None,
            // Kept apart from the database, as `docs/principles/keeping-observations.md` says.
            observations: Vec::new(),
            rev: current_rev(&tx)?,
        };
        tx.commit()?;
        Ok(records)
    }

    /// The records of one session, read in one transaction, so they're all as of one revision, as
    /// [`Store::list_records`] reads every other record.
    pub fn list_session_records(&self, session_id: &SessionId) -> Result<SessionRecords, StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let records = SessionRecords {
            session_id: session_id.clone(),
            subagents: subagents_of(&tx, session_id)?,
            proposals: proposals_of(&tx, session_id)?,
            replies_heard: replies_heard_of(&tx, session_id)?,
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
