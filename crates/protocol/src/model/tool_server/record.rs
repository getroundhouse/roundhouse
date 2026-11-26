use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{ToolServerAuth, ToolServerName, ToolServerUrl};
use crate::model::{Rev, ToolServerId};

/// A tool server the hub has of its own, which is an MCP server over HTTP, as a person added it, so every client shows
/// the same. It's in every project. What it knows the person by, such as a key, is in a file of the hub's, and never on
/// the record. Agents reach it through their worker, as `docs/decisions/hub-holds-tool-servers.md` says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolServer {
    pub id: ToolServerId,
    /// What the person calls it. Agents call it what [`ToolServerName::for_agents`] makes of that, which no two servers
    /// of a hub share.
    pub name: ToolServerName,
    /// Two servers can have the same, as for two accounts at it.
    pub url: ToolServerUrl,
    pub auth: ToolServerAuth,
    pub rev: Rev,
}

impl ToolServer {
    /// The order a hub lists tool servers in, and a client shows them: by name, in any case, then by ID.
    pub fn by_name(&self, other: &ToolServer) -> Ordering {
        let lowercase = |server: &ToolServer| server.name.as_str().to_lowercase();
        lowercase(self).cmp(&lowercase(other)).then_with(|| self.id.cmp(&other.id))
    }
}
