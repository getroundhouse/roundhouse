//! Workspaces.

use protocol::model::{BranchName, Branches, PullRequestFound, SetupStatus, Title, Workspace, WorkspaceStatus};
use protocol::model::{ClientRequestId, PORTS_PER_WORKSPACE, RepoId, Rev, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::instances::{InstanceStage, record_instance};
use super::made::{Made, inserted, made_by};
use super::{Store, StoreError, from_text, lock, now_ms, rev_of, text_of};

#[cfg(test)]
mod tests;

const FIRST_PORT_BASE: u16 = 55_000;

/// What recording a workspace's branches did.
pub struct BranchesSet {
    /// As they were recorded before, if they were.
    pub before: Option<Branches>,
    /// The workspace, if they changed it.
    pub workspace: Option<Workspace>,
}

/// Archiving or restoring a workspace, which its record says from when it's asked until it's done, or until it's put
/// back as it was, when it can't be done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Archive,
    Restore,
}

impl Move {
    /// Where it starts from.
    pub fn before(self) -> WorkspaceStatus {
        match self {
            Move::Archive => WorkspaceStatus::Ready,
            Move::Restore => WorkspaceStatus::Archived,
        }
    }

    /// What the record says meanwhile.
    pub fn during(self) -> WorkspaceStatus {
        match self {
            Move::Archive => WorkspaceStatus::Archiving,
            Move::Restore => WorkspaceStatus::Restoring,
        }
    }

    /// Where it ends once it's done.
    pub fn after(self) -> WorkspaceStatus {
        match self {
            Move::Archive => WorkspaceStatus::Archived,
            Move::Restore => WorkspaceStatus::Ready,
        }
    }
}

/// How archiving or restoring a workspace ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    Done,
    /// It was refused, or failed, so the workspace is as it was before.
    PutBack,
}

pub(super) const WORKSPACE_COLUMNS: &str = "id, repo_id, name, branch, base_ref, path, status, provider, instance_id, created_at_ms, archived_at_ms, setup, port_base, title, title_source, pull_request, user_active_at_ms, comment_draft, branches, pull_request_branch, worker_build";

/// The workspace's record as `conn` reads it, such as the writer inside a change, so its revision is that change's.
pub(super) fn workspace_on(conn: &Connection, id: &WorkspaceId) -> Result<Workspace, StoreError> {
    conn.query_row(
        &format!("SELECT {WORKSPACE_COLUMNS}, rev FROM workspaces WHERE id = ?"),
        [id.as_str()],
        workspace_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound { kind: "workspace", id: id.to_string() })
}

pub(super) fn workspace_from_row(row: &Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get::<_, String>(0)?.into(),
        repo_id: row.get::<_, String>(1)?.into(),
        name: row.get(2)?,
        branch: row.get::<_, String>(3)?.into(),
        base_ref: row.get(4)?,
        path: row.get(5)?,
        status: from_text(row.get(6)?)?,
        provider: row.get::<_, String>(7)?.into(),
        instance_id: row.get::<_, String>(8)?.into(),
        created_at_ms: row.get(9)?,
        archived_at_ms: row.get(10)?,
        setup: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or(SetupStatus::Unknown),
        port_base: row.get(12)?,
        title: row.get::<_, Option<String>>(13)?.as_deref().and_then(Title::tidied),
        title_source: from_text(row.get(14)?)?,
        // One that no longer reads, from a build whose idea of a pull request this one doesn't share, is looked for again.
        pull_request: row.get::<_, Option<String>>(15)?.and_then(|found| serde_json::from_str(&found).ok()),
        // Written only by this hub, as JSON. One it can't read is as good as none.
        comment_draft: row.get::<_, Option<String>>(17)?.and_then(|json| serde_json::from_str(&json).ok()),
        user_active_at_ms: row.get(16)?,
        // Read again by the next worker, so one that no longer reads is only missing until then.
        branches: row.get::<_, Option<String>>(18)?.and_then(|branches| serde_json::from_str(&branches).ok()),
        pull_request_branch: row.get::<_, Option<String>>(19)?.map(BranchName::from),
        worker_build: row.get(20)?,
        rev: rev_of(row, 21)?,
    })
}

/// What the hub found, as the column holds it: `None` until it has looked, which is SQL's NULL.
fn found_text(found: Option<&PullRequestFound>) -> Result<Option<String>, StoreError> {
    Ok(found.map(serde_json::to_string).transpose()?)
}

impl Store {
    /// Returns the workspace as stored, or `None` if its name is taken in its repository. Its instance is recorded with
    /// it, as being made if the workspace is, so it's recorded before its provider is asked for it.
    pub fn insert_workspace(
        &self,
        workspace: &Workspace,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Option<Made<Workspace>>, StoreError> {
        let mut conn = lock(&self.writer);
        if let Some(made) = made_by(&conn, client_request_id)? {
            return Ok(Some(Made::Earlier(made)));
        }
        let tx = conn.transaction()?;
        let added = tx.execute(
            &format!(
                "INSERT INTO workspaces ({WORKSPACE_COLUMNS}, client_request_id)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT (repo_id, name) DO NOTHING"
            ),
            params![
                workspace.id.as_str(),
                workspace.repo_id.as_str(),
                workspace.name,
                workspace.branch.as_str(),
                workspace.base_ref,
                workspace.path,
                text_of(&workspace.status),
                workspace.provider.as_str(),
                workspace.instance_id.as_str(),
                workspace.created_at_ms,
                workspace.archived_at_ms,
                serde_json::to_string(&workspace.setup)?,
                i64::from(workspace.port_base),
                workspace.title.as_deref(),
                text_of(&workspace.title_source),
                found_text(workspace.pull_request.as_ref())?,
                workspace.user_active_at_ms,
                workspace.comment_draft.as_ref().map(serde_json::to_string).transpose()?,
                workspace.branches.as_ref().map(serde_json::to_string).transpose()?,
                workspace.pull_request_branch.as_ref().map(BranchName::as_str),
                workspace.worker_build,
                client_request_id.map(ClientRequestId::as_str),
            ],
        )?;
        if added == 0 {
            return Ok(None);
        }
        let stage =
            if workspace.status == WorkspaceStatus::Creating { InstanceStage::Creating } else { InstanceStage::Ready };
        record_instance(&tx, &workspace.instance_id, &workspace.provider, stage, workspace.created_at_ms)?;
        let made = inserted(&tx, workspace.id.as_str())?;
        tx.commit()?;
        Ok(Some(made))
    }

    /// Returns the revision of the removal, or `None` if there was nothing to remove.
    pub fn delete_workspace(&self, id: &WorkspaceId) -> Result<Option<Rev>, StoreError> {
        self.delete("workspaces", id.as_str())
    }

    /// With its branches as its worktree was made, so a ready workspace says which it's on from the start.
    pub fn workspace_ready(
        &self,
        id: &WorkspaceId,
        path: &str,
        branch: &BranchName,
        base_ref: &str,
        branches: Option<&Branches>,
    ) -> Result<Workspace, StoreError> {
        lock(&self.writer).execute(
            "UPDATE workspaces SET path = ?, branch = ?, base_ref = ?, status = ?, branches = ? WHERE id = ?",
            params![
                path,
                branch.as_str(),
                base_ref,
                text_of(&WorkspaceStatus::Ready),
                branches.map(serde_json::to_string).transpose()?,
                id.as_str()
            ],
        )?;
        self.get_workspace(id)
    }

    /// Gives a workspace still being made another name, and the branch of the same name, as when git already has a
    /// branch or directory by the one it had. Only before it's ready, since a workspace's name is its worktree's
    /// directory once it has one. Returns the workspace renamed, or `None` if another in its repository has the name,
    /// or it's no longer being made.
    pub fn rename_unready_workspace(&self, id: &WorkspaceId, name: &str) -> Result<Option<Workspace>, StoreError> {
        let changed = lock(&self.writer).execute(
            "UPDATE OR IGNORE workspaces SET name = ?1, branch = ?1 WHERE id = ?2 AND status = ?3",
            params![name, id.as_str(), text_of(&WorkspaceStatus::Creating)],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_workspace(id).map(Some)
    }

    /// Records that the workspace is being archived or restored, if it's where that starts from, so only one request
    /// can do it at once. Returns the workspace if it was.
    pub fn start_move(&self, id: &WorkspaceId, moving: Move) -> Result<Option<Workspace>, StoreError> {
        let changed = lock(&self.writer).execute(
            "UPDATE workspaces SET status = ? WHERE id = ? AND status = ?",
            params![text_of(&moving.during()), id.as_str(), text_of(&moving.before())],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_workspace(id).map(Some)
    }

    /// Records that archiving or restoring the workspace is done, or puts it back as it was before. When it was
    /// archived moves only as archiving is done, and goes only as restoring is.
    pub fn end_move(&self, id: &WorkspaceId, moving: Move, ended: Ended) -> Result<Workspace, StoreError> {
        let conn = lock(&self.writer);
        match ended {
            Ended::Done => {
                let archived_at = (moving == Move::Archive).then(now_ms);
                conn.execute(
                    "UPDATE workspaces SET status = ?, archived_at_ms = ? WHERE id = ?",
                    params![text_of(&moving.after()), archived_at, id.as_str()],
                )?;
            }
            // Only from where the move left it, so nothing else is undone.
            Ended::PutBack => {
                conn.execute(
                    "UPDATE workspaces SET status = ? WHERE id = ? AND status = ?",
                    params![text_of(&moving.before()), id.as_str(), text_of(&moving.during())],
                )?;
            }
        }
        workspace_on(&conn, id)
    }

    /// Every workspace a hub was archiving or restoring when it stopped, and which it was doing. Their records go on
    /// saying so until the next hub has settled each.
    pub fn moves_left(&self) -> Result<Vec<(WorkspaceId, Move)>, StoreError> {
        let conn = lock(&self.reader);
        let mut left = Vec::new();
        for moving in [Move::Archive, Move::Restore] {
            let mut statement =
                conn.prepare_cached("SELECT id FROM workspaces WHERE status = ? ORDER BY created_at_ms")?;
            let ids = statement.query_map([text_of(&moving.during())], |row| row.get::<_, String>(0))?;
            for id in ids {
                left.push((WorkspaceId::from(id?), moving));
            }
        }
        Ok(left)
    }

    /// Records what the hub found of the workspace's pull request, and on which branch it looked. Returns the workspace
    /// only when that changed it, so finding the same again moves no revision and announces nothing.
    pub fn set_workspace_pull_request(
        &self,
        id: &WorkspaceId,
        found: &PullRequestFound,
        branch: Option<&BranchName>,
    ) -> Result<Option<Workspace>, StoreError> {
        let found = found_text(Some(found))?;
        let changed = lock(&self.writer).execute(
            "UPDATE workspaces SET pull_request = ?1, pull_request_branch = ?2
                 WHERE id = ?3 AND (pull_request IS NOT ?1 OR pull_request_branch IS NOT ?2)",
            params![found, branch.map(BranchName::as_str), id.as_str()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_workspace(id).map(Some)
    }

    /// Records what the workspace's worker read of its branches. Says what they were before, and returns the workspace
    /// only when that changed it, as [`Store::set_workspace_pull_request`] does.
    pub fn set_workspace_branches(&self, id: &WorkspaceId, branches: &Branches) -> Result<BranchesSet, StoreError> {
        let text = serde_json::to_string(branches)?;
        // Both under the writer's lock, so nothing records others in between.
        let (before, changed) = {
            let conn = lock(&self.writer);
            let select = "SELECT branches FROM workspaces WHERE id = ?";
            let before: Option<String> = conn.query_row(select, [id.as_str()], |row| row.get(0))?;
            let update = "UPDATE workspaces SET branches = ?1 WHERE id = ?2 AND branches IS NOT ?1";
            (before, conn.execute(update, params![text, id.as_str()])? > 0)
        };
        let before = before.and_then(|before| serde_json::from_str(&before).ok());
        Ok(BranchesSet { before, workspace: if changed { Some(self.get_workspace(id)?) } else { None } })
    }

    pub fn set_workspace_setup(&self, id: &WorkspaceId, setup: SetupStatus) -> Result<Workspace, StoreError> {
        lock(&self.writer).execute(
            "UPDATE workspaces SET setup = ? WHERE id = ?",
            params![serde_json::to_string(&setup)?, id.as_str()],
        )?;
        self.get_workspace(id)
    }

    /// The start of ten ports nothing else has. Never reused, so a restored workspace keeps its own.
    pub fn next_port_base(&self) -> Result<u16, StoreError> {
        let highest: u16 =
            lock(&self.reader).query_row("SELECT COALESCE(MAX(port_base), 0) FROM workspaces", [], |row| row.get(0))?;
        let next = if highest == 0 { FIRST_PORT_BASE } else { highest + PORTS_PER_WORKSPACE };
        // Past the top of the range, start again rather than fail. A clash is possible, but only
        // after thousands of workspaces, and it only matters if both are running a server.
        Ok(if next + PORTS_PER_WORKSPACE > 65_000 { FIRST_PORT_BASE } else { next })
    }

    /// A setup that was running when the hub stopped never finished, and its worker has gone.
    pub fn fail_interrupted_setups(&self) -> Result<(), StoreError> {
        let running = serde_json::to_string(&SetupStatus::Running)?;
        let failed = serde_json::to_string(&SetupStatus::Failed { code: None })?;
        lock(&self.writer).execute("UPDATE workspaces SET setup = ? WHERE setup = ?", params![failed, running])?;
        Ok(())
    }

    pub fn set_workspace_branch(&self, id: &WorkspaceId, branch: &BranchName) -> Result<(), StoreError> {
        let update = "UPDATE workspaces SET branch = ? WHERE id = ?";
        lock(&self.writer).execute(update, params![branch.as_str(), id.as_str()])?;
        Ok(())
    }

    pub fn get_workspace(&self, id: &WorkspaceId) -> Result<Workspace, StoreError> {
        workspace_on(&lock(&self.reader), id)
    }

    pub fn list_workspaces(&self, include_archived: bool) -> Result<Vec<Workspace>, StoreError> {
        let conn = lock(&self.reader);
        let filter = if include_archived { "" } else { "WHERE status != 'archived'" };
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {WORKSPACE_COLUMNS}, rev FROM workspaces {filter} ORDER BY created_at_ms"
        ))?;
        let rows = statement.query_map([], workspace_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    pub fn names_in_repo(&self, repo_id: &RepoId) -> Result<Vec<String>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached("SELECT name FROM workspaces WHERE repo_id = ?")?;
        let rows = statement.query_map([repo_id.as_str()], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    /// A workspace left half-made by a crash has no usable worktree or sessions.
    pub fn take_unfinished_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        let conn = lock(&self.writer);
        let mut statement =
            conn.prepare_cached(&format!("SELECT {WORKSPACE_COLUMNS}, rev FROM workspaces WHERE status = ?"))?;
        let creating = text_of(&WorkspaceStatus::Creating);
        let rows = statement.query_map([&creating], workspace_from_row)?;
        let workspaces = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        conn.execute("DELETE FROM workspaces WHERE status = ?", [&creating])?;
        Ok(workspaces)
    }
}
