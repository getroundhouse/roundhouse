use super::super::{Store, StoreError, lock};
use protocol::model::{SessionId, WorkspaceId};
use protocol::worker::ToolsToken;
use rusqlite::{OptionalExtension, params};

impl Store {
    /// The port the workspace's worker last served its tools on, or `None` if it never has.
    pub fn tools_port(&self, id: &WorkspaceId) -> Result<Option<u16>, StoreError> {
        Ok(lock(&self.reader)
            .query_row("SELECT port FROM tools_ports WHERE workspace_id = ?", [id.as_str()], |row| row.get(0))
            .optional()?)
    }

    pub fn set_tools_port(&self, id: &WorkspaceId, port: u16) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            "INSERT INTO tools_ports (workspace_id, port) VALUES (?1, ?2)
                 ON CONFLICT (workspace_id) DO UPDATE SET port = excluded.port",
            params![id.as_str(), port],
        )?;
        Ok(())
    }

    /// The session's token for its workspace's tools. The first time, that's `made`, which is kept from then on.
    pub fn tools_token(&self, id: &SessionId, made: &ToolsToken) -> Result<ToolsToken, StoreError> {
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO tools_tokens (session_id, token) VALUES (?, ?) ON CONFLICT (session_id) DO NOTHING",
            params![id.as_str(), made.as_str()],
        )?;
        let token: String =
            conn.query_row("SELECT token FROM tools_tokens WHERE session_id = ?", [id.as_str()], |row| row.get(0))?;
        Ok(ToolsToken::new(token))
    }
}
