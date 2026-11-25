use std::sync::Arc;

use protocol::client::tool_servers::ToolServerAdd;
use protocol::client::{ApiError, HubNotice};
use protocol::model::{RecordKey, ToolServer, ToolServerId};
use tokio::sync::watch;
use tracing::error;

use super::super::{Hub, off_thread};
use crate::tool_servers::{SentToolServers, ToolServerChange, ToolServerRemoved};

impl Hub {
    /// Adds a tool server, as `ToolServerAdd` says, and tells every client of its record.
    pub(crate) async fn tool_server_add(&self, asked: ToolServerAdd) -> Result<ToolServer, ApiError> {
        let servers = Arc::clone(&self.tool_servers);
        Ok(self.announce_tool_server(off_thread(move || servers.add(asked)).await?))
    }

    /// Forgets a tool server and its key or its sign-in, as `ToolServerRemove` says, and tells every client it's gone.
    pub(crate) async fn tool_server_remove(&self, id: ToolServerId) -> Result<ToolServer, ApiError> {
        let servers = Arc::clone(&self.tool_servers);
        let ToolServerRemoved { server, rev } = off_thread(move || servers.remove(&id)).await?;
        self.notify(HubNotice::Removed { record: RecordKey::ToolServer { id: server.id.clone() }, rev });
        Ok(server)
    }

    /// Makes the tool servers' files and their rows agree, as the hub starts, as [`Hub::tidy_logins`] says of the
    /// logins'.
    pub(in super::super) async fn tidy_tool_servers(&self) {
        let servers = Arc::clone(&self.tool_servers);
        if let Err(e) = off_thread(move || servers.tidy()).await {
            error!(
                name: "tool_servers.tidy_failed",
                error = e.message,
                "Couldn't make the tool servers' files and the tool servers agree"
            );
        }
    }

    /// The tool servers each worker's link sets on its worker, as
    /// [`ToolServers::sent`](crate::tool_servers::ToolServers::sent) says.
    pub(crate) fn sent_tool_servers(&self) -> watch::Receiver<SentToolServers> {
        self.tool_servers.sent()
    }

    pub(super) fn announce_tool_server(&self, ToolServerChange { server, changed }: ToolServerChange) -> ToolServer {
        if changed {
            self.notify(HubNotice::ToolServer(server.clone()));
        }
        server
    }
}
