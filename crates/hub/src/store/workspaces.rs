//! Workspaces.

use protocol::model::{ClientRequestId, RepoId, Rev, WorkspaceId};
use protocol::model::{PullRequestFound, SetupStatus, Title, Workspace, WorkspaceStatus};
use rusqlite::{OptionalExtension, Row, params};

use super::instances::{InstanceStage, record_instance};
use super::made::{Made, inserted, made_by};
use super::{Store, StoreError, from_text, lock, now_ms, rev_of, text_of};

#[cfg(test)]
mod tests;

const FIRST_PORT_BASE: u16 = 55_000;

const PORTS_PER_WORKSPACE: u16 = 10;

pub(super) const WORKSPACE_COLUMNS: &str = "id, repo_id, name, branch, base_ref, path, status, provider, instance_id, created_at_ms, archived_at_ms, setup, port_base, title, title_source, pull_request, user_active_at_ms, comment_draft";

pub(super) fn workspace_from_row(row: &Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get::<_, String>(0)?.into(),
        repo_id: row.get::<_, String>(1)?.into(),
        name: row.get(2)?,
        branch: row.get(3)?,
        base_ref: row.get(4)?,
        path: row.get(5)?,
        status: from_text(row.get(6)?)?,
        provider: row.get(7)?,
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
        rev: rev_of(row, 18)?,
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
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT (repo_id, name) DO NOTHING"
            ),
            params![
                workspace.id.as_str(),
                workspace.repo_id.as_str(),
                workspace.name,
                workspace.branch,
                workspace.base_ref,
                workspace.path,
                text_of(&workspace.status),
                workspace.provider,
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

    pub fn workspace_ready(
        &self,
        id: &WorkspaceId,
        path: &str,
        branch: &str,
        base_ref: &str,
    ) -> Result<Workspace, StoreError> {
        lock(&self.writer).execute(
            "UPDATE workspaces SET path = ?, branch = ?, base_ref = ?, status = ? WHERE id = ?",
            params![path, branch, base_ref, text_of(&WorkspaceStatus::Ready), id.as_str()],
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

    pub fn set_workspace_archived(&self, id: &WorkspaceId, archived: bool) -> Result<Workspace, StoreError> {
        let (status, archived_at) =
            if archived { (WorkspaceStatus::Archived, Some(now_ms())) } else { (WorkspaceStatus::Ready, None) };
        lock(&self.writer).execute(
            "UPDATE workspaces SET status = ?, archived_at_ms = ? WHERE id = ?",
            params![text_of(&status), archived_at, id.as_str()],
        )?;
        self.get_workspace(id)
    }

    /// Records what the hub found of the workspace's pull request. Returns the workspace only when that changed it, so
    /// finding the same again moves no revision and announces nothing.
    pub fn set_workspace_pull_request(
        &self,
        id: &WorkspaceId,
        found: &PullRequestFound,
    ) -> Result<Option<Workspace>, StoreError> {
        let found = found_text(Some(found))?;
        let changed = lock(&self.writer).execute(
            "UPDATE workspaces SET pull_request = ?1 WHERE id = ?2 AND pull_request IS NOT ?1",
            params![found, id.as_str()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_workspace(id).map(Some)
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

    pub fn set_workspace_branch(&self, id: &WorkspaceId, branch: &str) -> Result<(), StoreError> {
        lock(&self.writer).execute("UPDATE workspaces SET branch = ? WHERE id = ?", params![branch, id.as_str()])?;
        Ok(())
    }

    pub fn get_workspace(&self, id: &WorkspaceId) -> Result<Workspace, StoreError> {
        lock(&self.reader)
            .query_row(
                &format!("SELECT {WORKSPACE_COLUMNS}, rev FROM workspaces WHERE id = ?"),
                [id.as_str()],
                workspace_from_row,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "workspace", id: id.to_string() })
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
