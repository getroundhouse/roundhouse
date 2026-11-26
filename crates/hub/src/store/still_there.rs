//! Whether one of the hub's records is still there, asked as a change to what goes with it takes its revision, so what
//! the hub keeps in memory of a record that's gone, such as what a worker said of a workspace removed meanwhile, is
//! never kept.

use std::collections::HashSet;
use std::hash::Hash;

use protocol::model::{
    AccountId, ChildWorktreeId, GithubAccountId, GithubAppId, RepoId, Rev, SessionId, WaitId, WorkspaceId,
    WorkspaceStatus,
};
use rusqlite::{Connection, OptionalExtension, Params, params};

use super::database::{bump, lock};
use super::{OneValue, Store, StoreError, text_of};

/// One of the hub's records, which something the hub keeps in memory goes with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Record {
    Workspace(WorkspaceId),
    /// A workspace's own worktree, which is there while the workspace is, unless it's archived.
    Worktree(WorkspaceId),
    /// Another worktree made for a workspace's work, whose record stays once it's removed, until its workspace goes.
    ChildWorktree(ChildWorktreeId),
    Session(SessionId),
    /// A wait, which is there until it ends.
    Wait(WaitId),
    Account(AccountId),
    Repo(RepoId),
    GithubAccount(GithubAccountId),
    App(GithubAppId),
}

impl Store {
    /// A revision for a change to what goes with `record`, as [`Store::next_rev`] gives, taken only while the record is
    /// still there, which is asked under the same hold of the writer, so no write removes or archives it in between.
    /// `None`, with no revision taken, for a record that's gone, and for a workspace's worktree once the workspace is
    /// archived. With no record, as for what the hub keeps of itself, it's only the revision.
    pub(crate) fn next_rev_while_there(&self, record: Option<&Record>) -> Result<Option<Rev>, StoreError> {
        let conn = lock(&self.writer);
        if let Some(record) = record
            && !there(&conn, record)?
        {
            return Ok(None);
        }
        Ok(Some(Rev::new(bump(&conn, "rev")?)))
    }
}

/// Every record the database has, of each kind something the hub keeps in memory can go with, as the hub reads them as
/// it starts, to forget what goes with one that's gone. Each is there as [`Store::next_rev_while_there`] says.
pub(crate) struct RecordsThere {
    workspaces: HashSet<WorkspaceId>,
    /// The workspaces that aren't archived, whose own worktree is there.
    worktrees: HashSet<WorkspaceId>,
    child_worktrees: HashSet<ChildWorktreeId>,
    sessions: HashSet<SessionId>,
    /// Those that haven't ended.
    waits: HashSet<WaitId>,
    accounts: HashSet<AccountId>,
    repos: HashSet<RepoId>,
    github_accounts: HashSet<GithubAccountId>,
    apps: HashSet<GithubAppId>,
}

impl RecordsThere {
    /// Whether `record` was there as they were read.
    pub(crate) fn has(&self, record: &Record) -> bool {
        match record {
            Record::Workspace(id) => self.workspaces.contains(id),
            Record::Worktree(id) => self.worktrees.contains(id),
            Record::ChildWorktree(id) => self.child_worktrees.contains(id),
            Record::Session(id) => self.sessions.contains(id),
            Record::Wait(id) => self.waits.contains(id),
            Record::Account(id) => self.accounts.contains(id),
            Record::Repo(id) => self.repos.contains(id),
            Record::GithubAccount(id) => self.github_accounts.contains(id),
            Record::App(id) => self.apps.contains(id),
        }
    }
}

impl Store {
    /// Every record that's there, as [`RecordsThere`] says.
    pub(crate) fn records_there(&self) -> Result<RecordsThere, StoreError> {
        let conn = lock(&self.reader);
        let archived = text_of(&WorkspaceStatus::Archived);
        Ok(RecordsThere {
            workspaces: ids(&conn, "SELECT id FROM workspaces", [])?,
            worktrees: ids(&conn, "SELECT id FROM workspaces WHERE status != ?", [archived])?,
            child_worktrees: ids(&conn, "SELECT id FROM child_worktrees", [])?,
            sessions: ids(&conn, "SELECT id FROM sessions", [])?,
            waits: ids(&conn, "SELECT id FROM waits WHERE ended_at_ms IS NULL", [])?,
            accounts: ids(&conn, "SELECT id FROM accounts", [])?,
            repos: ids(&conn, "SELECT id FROM repos", [])?,
            github_accounts: ids(&conn, "SELECT id FROM github_accounts", [])?,
            apps: conn
                .prepare("SELECT id FROM github_apps")?
                .query_map([], OneValue::read::<u64>)?
                .map(|id| id.map(GithubAppId::new))
                .collect::<rusqlite::Result<_>>()?,
        })
    }
}

/// The ids `query` finds, each as one of the hub's ids.
fn ids<T: From<String> + Eq + Hash>(
    conn: &Connection,
    query: &str,
    params: impl Params,
) -> rusqlite::Result<HashSet<T>> {
    conn.prepare(query)?.query_map(params, OneValue::read::<String>)?.map(|id| id.map(T::from)).collect()
}

/// Whether `record` is there, by one read of its row by its key.
fn there(conn: &Connection, record: &Record) -> rusqlite::Result<bool> {
    match record {
        Record::Workspace(id) => found(conn, "SELECT 1 FROM workspaces WHERE id = ?", [id.as_str()]),
        Record::Worktree(id) => {
            let archived = text_of(&WorkspaceStatus::Archived);
            found(conn, "SELECT 1 FROM workspaces WHERE id = ? AND status != ?", params![id.as_str(), archived])
        }
        Record::ChildWorktree(id) => found(conn, "SELECT 1 FROM child_worktrees WHERE id = ?", [id.as_str()]),
        Record::Session(id) => found(conn, "SELECT 1 FROM sessions WHERE id = ?", [id.as_str()]),
        Record::Wait(id) => found(conn, "SELECT 1 FROM waits WHERE id = ? AND ended_at_ms IS NULL", [id.as_str()]),
        Record::Account(id) => found(conn, "SELECT 1 FROM accounts WHERE id = ?", [id.as_str()]),
        Record::Repo(id) => found(conn, "SELECT 1 FROM repos WHERE id = ?", [id.as_str()]),
        Record::GithubAccount(id) => found(conn, "SELECT 1 FROM github_accounts WHERE id = ?", [id.as_str()]),
        Record::App(id) => found(conn, "SELECT 1 FROM github_apps WHERE id = ?", [id.get()]),
    }
}

/// Whether `query` finds a row.
fn found(conn: &Connection, query: &str, params: impl Params) -> rusqlite::Result<bool> {
    Ok(conn.prepare_cached(query)?.query_row(params, |_| Ok(())).optional()?.is_some())
}
