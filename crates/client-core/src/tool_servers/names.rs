use std::collections::HashMap;
use std::sync::Arc;

use protocol::capability::setup::ServerName;
use protocol::model::{ToolServer, ToolServerName};

/// What the person calls each of the hub's tool servers, by what agents call it, as the hub's records are now. So a call
/// made before a server was removed shows the name its agent called it by. Cloning it shares it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolServerNames(Arc<HashMap<ServerName, ToolServerName>>);

impl ToolServerNames {
    pub(crate) fn of(servers: &[ToolServer]) -> Self {
        let names = servers.iter().map(|server| (server.name.for_agents(), server.name.clone()));
        Self(Arc::new(names.collect()))
    }

    /// `server`, an agent's name for a server, as it's shown: by the person's name for it where it's one of the hub's, and
    /// otherwise as it is. A server of an agent's CLI's own that has the same name as one of the hub's shows as the hub's,
    /// since nothing else tells the two apart.
    pub fn shown<'a>(&'a self, server: &'a ServerName) -> &'a str {
        self.0.get(server).map_or(server.as_str(), ToolServerName::as_str)
    }
}
