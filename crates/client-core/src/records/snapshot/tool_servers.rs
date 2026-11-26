//! The tool servers the hub has of its own, as views read them, which [`crate::tool_servers`] says what's shown of.

use protocol::model::{ToolServer, ToolServerId, ToolServerUrl};

use super::Snapshot;
use crate::tool_servers::ToolServerNames;

impl Snapshot {
    /// Every tool server the hub has of its own, by name. Empty until they've arrived.
    pub fn tool_servers(&self) -> &[ToolServer] {
        &self.records.tool_servers
    }

    /// What the person calls each, where an agent's name for it would show.
    pub fn tool_server_names(&self) -> &ToolServerNames {
        &self.records.tool_server_names
    }

    /// The record of the tool server `id`, until it's removed.
    pub fn tool_server(&self, id: &ToolServerId) -> Option<&ToolServer> {
        self.records.tool_servers.iter().find(|server| server.id == *id)
    }

    /// The first tool server, by name, at the URL a person `typed`, as the hub would keep it, for a client to say it's
    /// there already. Two can be at one URL, so it stops nobody adding another. `edited` is the one it's typed for,
    /// which isn't another.
    pub fn tool_server_at(&self, typed: &str, edited: Option<&ToolServerId>) -> Option<&ToolServer> {
        let url = ToolServerUrl::typed(typed).ok()?;
        self.records.tool_servers.iter().find(|server| server.url == url && Some(&server.id) != edited)
    }
}
