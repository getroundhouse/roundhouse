use std::collections::HashMap;
use std::sync::Arc;

use protocol::capability::setup::{ServerName, ToolServerState};
use protocol::model::{ToolServer, ToolServerName};

/// What the person calls each of the hub's tool servers, by what a chat's agent calls it, as the hub's records are now.
/// Cloning it shares it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolServerNames(Arc<HashMap<ServerName, ToolServerName>>);

impl ToolServerNames {
    /// Of `servers`, the hub's, for a chat whose agent's run says it has `said`, where it has said. Each name the run says
    /// it has is the server its worker says it is, so a server renamed since the agent started still shows the person's
    /// name for it, and one of its CLI's own shows as it is, whatever it's called. A name it doesn't say, as of a call
    /// from an earlier run, or before it has said, is taken as the one agents are given for a server now.
    pub(crate) fn of(servers: &[ToolServer], said: Option<&[ToolServerState]>) -> Self {
        let mut names: HashMap<_, _> =
            servers.iter().map(|server| (server.name.for_agents(), server.name.clone())).collect();
        for said in said.into_iter().flatten() {
            let theirs = said.hubs.as_ref().and_then(|id| servers.iter().find(|server| server.id == *id));
            match theirs {
                Some(server) => names.insert(said.name.clone(), server.name.clone()),
                // One of the CLI's own, or one the hub has since removed, which shows as the agent calls it.
                None => names.remove(&said.name),
            };
        }
        Self(Arc::new(names))
    }

    /// `server`, an agent's name for a server, as it's shown: by the person's name for it where it's one of the hub's, and
    /// otherwise as it is.
    pub fn shown<'a>(&'a self, server: &'a ServerName) -> &'a str {
        self.0.get(server).map_or(server.as_str(), ToolServerName::as_str)
    }
}
