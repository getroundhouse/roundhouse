use super::super::{Store, StoreError, lock};
use protocol::model::{SessionId, Workspace, WorkspaceId};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Records which build the workspace's worker is, `None` once it's the hub's own. Returns the workspace only when
    /// that changed it, as [`Store::set_workspace_branches`] does, so saying the same again announces nothing.
    pub fn set_workspace_worker_build(
        &self,
        id: &WorkspaceId,
        build: Option<&str>,
    ) -> Result<Option<Workspace>, StoreError> {
        let update = "UPDATE workspaces SET worker_build = ?1 WHERE id = ?2 AND worker_build IS NOT ?1";
        let changed = lock(&self.writer).execute(update, params![build, id.as_str()])?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_workspace(id).map(Some)
    }

    /// Forgets which build every workspace's worker was, as the hub starts, before it carries on with any worker an
    /// earlier hub left, since only one it carries on with can be another build than its own.
    pub fn forget_worker_builds(&self) -> Result<(), StoreError> {
        lock(&self.writer).execute("UPDATE workspaces SET worker_build = NULL WHERE worker_build IS NOT NULL", [])?;
        Ok(())
    }

    /// The build the session's agent last started on, as its newest `AgentStarted` says. `None` before its first, and
    /// when that didn't say, as from a hub from before it recorded builds.
    pub fn last_agent_build(&self, id: &SessionId) -> Result<Option<String>, StoreError> {
        let select = "SELECT json_extract(body, '$.build') FROM events \
                      WHERE session_id = ? AND kind = 'agent_started' ORDER BY seq DESC LIMIT 1";
        let build = lock(&self.reader).query_row(select, [id.as_str()], |row| row.get(0)).optional()?;
        Ok(build.flatten())
    }
}
