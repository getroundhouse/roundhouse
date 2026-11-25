//! The calls about the tool servers a hub has of its own: adding one and removing it. No client makes them: "MCP
//! servers" in `docs/spec/settings.md` plans the page that will. A refusal is worded as any other is, by
//! [`crate::refusals`], since the hub's message says what's wrong with what the person typed.

use protocol::client::tool_servers;
use protocol::model::{ToolServer, ToolServerAuthGiven, ToolServerId};

use super::{Client, Result};

impl Client {
    /// Adds the tool server `id`, a new one from [`new_tool_server_id`], called `name`, at `url`, which is to know the
    /// person as `auth` says, and answers with its record. See [`tool_servers::ToolServerAdd`].
    pub fn tool_server_add(
        &self,
        id: &ToolServerId,
        name: &str,
        url: &str,
        auth: ToolServerAuthGiven,
    ) -> Result<ToolServer> {
        self.call(tool_servers::ToolServerAdd { id: id.clone(), name: name.to_string(), url: url.to_string(), auth })
    }

    /// Has the hub forget the tool server `id` and its key, and answers with its record as it last was, which the feed
    /// then says has been removed. See [`tool_servers::ToolServerRemove`].
    pub fn tool_server_remove(&self, id: &ToolServerId) -> Result<ToolServer> {
        self.call(tool_servers::ToolServerRemove { id: id.clone() })
    }
}

/// A new ID for a tool server, chosen by the client, so adding it again is the same server.
pub fn new_tool_server_id() -> ToolServerId {
    ToolServerId::from(uuid::Uuid::new_v4().to_string())
}
