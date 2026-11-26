use std::sync::Arc;

use protocol::client::ApiError;
use protocol::worker::{AskForToolAccess, ToolAccess};

use super::super::{Hub, off_thread};
use crate::tool_servers::Wanted;

impl Hub {
    /// Keeps what access a tool server wants, as `ToolServers::wants_access` says, and tells every client when that
    /// changed its record. The person is told there, and isn't asked during the turn, since signing in is no approval:
    /// it's done once, on the server's page, for every chat. *Unconfirmed.*
    pub(crate) async fn tool_server_wants_access(&self, asked: AskForToolAccess) -> Result<ToolAccess, ApiError> {
        let servers = Arc::clone(&self.tool_servers);
        let Wanted { access, change } = off_thread(move || servers.wants_access(&asked)).await?;
        if let Some(change) = change {
            self.announce_tool_server(change);
        }
        Ok(access)
    }
}
