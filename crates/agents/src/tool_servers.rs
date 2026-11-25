use protocol::capability::setup::ServerName;
use protocol::worker::{TOOLS_SERVER, ToolsToken};
use std::path::PathBuf;

/// Where an agent reaches the tool servers its worker serves it, over MCP: the one of the worker's own tools, and each
/// of the hub's, which the worker passes calls on to.
#[derive(Clone, Debug)]
pub struct ToolServers {
    /// The worker's own tools, such as `http://127.0.0.1:50123/mcp`.
    pub tools_url: String,
    pub hubs: Vec<HubToolServer>,
    /// Says whose a call is, sent with every request to any of them.
    pub token: ToolsToken,
    /// Where the agent's CLI is told about them, written as it starts and removed once it has exited. It holds the
    /// token, so on Unix only its user can read it, and on Windows whoever its folder lets. Kept out of the worktree,
    /// where the agent and git would see it.
    pub config: PathBuf,
}

/// One of the hub's tool servers, where its worker serves it, which is never the server's own URL.
#[derive(Clone, Debug)]
pub struct HubToolServer {
    pub name: ServerName,
    pub url: String,
}

impl ToolServers {
    /// Each server's name and URL, the worker's own first. No two have one name, since the hub names its own unlike each
    /// other and never [`TOOLS_SERVER`]. The hub's names are also ones every CLI takes, with no dot, which would be a
    /// level in a dotted key of Codex's config.
    pub(crate) fn named(&self) -> impl Iterator<Item = (&str, &str)> {
        debug_assert!(self.hubs.iter().all(|server| server.name.as_str() != TOOLS_SERVER));
        let hubs = self.hubs.iter().map(|server| (server.name.as_str(), server.url.as_str()));
        std::iter::once((TOOLS_SERVER, self.tools_url.as_str())).chain(hubs)
    }

    /// The `Authorization` header each request to any of them carries.
    pub(crate) fn authorization(&self) -> String {
        format!("Bearer {}", self.token.as_str())
    }
}
