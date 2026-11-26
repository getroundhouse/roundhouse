use protocol::capability::setup::{ServerName, ServerState, ToolServerState};
use protocol::model::ToolServerId;
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
    pub id: ToolServerId,
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

    /// `said`, what an adapter says of the servers the agent has, with which of them are the hub's, and after them each
    /// of the hub's it says nothing of, so a report says whole which of the hub's the agent was launched with.
    ///
    /// They're told by name, since a CLI says nothing else of one. That's right of a server its own settings have under
    /// the same name too, as each CLI's reference in `docs/reference/` says: Claude uses the hub's in its place, and
    /// Codex takes the hub's URL over its own. Antigravity uses its own instead, and reports none.
    pub fn saying_which_are_the_hubs(&self, mut said: Vec<ToolServerState>) -> Vec<ToolServerState> {
        for server in &mut said {
            let hubs = self.hubs.iter().find(|hubs| hubs.name == server.name);
            server.hubs = hubs.map(|hubs| hubs.id.clone());
        }
        for hubs in &self.hubs {
            if !said.iter().any(|server| server.name == hubs.name) {
                said.push(ToolServerState {
                    name: hubs.name.clone(),
                    hubs: Some(hubs.id.clone()),
                    connected_to: None,
                    state: ServerState::Unknown,
                    error: None,
                    tools: None,
                });
            }
        }
        said
    }

    /// The `Authorization` header each request to any of them carries.
    pub(crate) fn authorization(&self) -> String {
        format!("Bearer {}", self.token.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(name: &str, hubs: Option<&str>, state: ServerState) -> ToolServerState {
        ToolServerState {
            name: name.into(),
            hubs: hubs.map(Into::into),
            connected_to: None,
            state,
            error: None,
            tools: None,
        }
    }

    #[test]
    fn what_an_agent_says_of_its_servers_says_which_are_the_hubs_and_each_of_those_it_was_launched_with() {
        let hubs = |id: &str, name: &str| HubToolServer { id: id.into(), name: name.into(), url: String::new() };
        let launched = ToolServers {
            tools_url: String::new(),
            hubs: vec![hubs("t-1", "Linear"), hubs("t-2", "Runbooks")],
            token: ToolsToken::new("token"),
            config: PathBuf::new(),
        };
        let agents = vec![
            said("linear", None, ServerState::Failed),
            said("Linear", None, ServerState::Connected),
            said("docs", None, ServerState::Starting),
        ];

        let servers = launched.saying_which_are_the_hubs(agents);

        let whole = [
            said("linear", None, ServerState::Failed),
            said("Linear", Some("t-1"), ServerState::Connected),
            said("docs", None, ServerState::Starting),
            said("Runbooks", Some("t-2"), ServerState::Unknown),
        ];
        assert_eq!(servers, whole);
    }
}
