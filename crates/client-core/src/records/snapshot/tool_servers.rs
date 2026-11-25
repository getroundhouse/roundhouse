//! The tool servers the hub has of its own, as views read them.

use protocol::model::{ToolServer, ToolServerId, ToolServerUrl};

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

    /// The first tool server, by name, at the URL a person `typed`, as the hub would keep it, for a client to say it's
    /// there already. Two can be at one URL, so it stops nobody adding another.
    pub fn tool_server_at(&self, typed: &str) -> Option<&ToolServer> {
        let url = ToolServerUrl::typed(typed).ok()?;
        self.records.tool_servers.iter().find(|server| server.url == url)
    }
}
