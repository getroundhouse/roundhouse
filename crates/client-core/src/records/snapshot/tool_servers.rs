//! The tool servers the hub has of its own, as views read them. No view does: "MCP servers" in `docs/spec/settings.md`
//! plans the page that will.

use protocol::model::{ToolServer, ToolServerId};

use super::Snapshot;

impl Snapshot {
    /// Every tool server the hub has of its own, by name. Empty until they've arrived.
    pub fn tool_servers(&self) -> &[ToolServer] {
        &self.records.tool_servers
    }

    /// The record of the tool server `id`, until it's removed.
    pub fn tool_server(&self, id: &ToolServerId) -> Option<&ToolServer> {
        self.records.tool_servers.iter().find(|server| server.id == *id)
    }
}
