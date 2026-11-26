use protocol::capability::setup::ToolServerState;
use protocol::model::Session;

use super::{SignIn, Stands};
use crate::records::Snapshot;

/// The tool servers a chat has, as its card lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatToolServers {
    /// The hub's, by the person's names for them, then any the agent has that the hub has removed.
    pub hubs: Vec<ChatToolServer>,
    /// The ones the agent's CLI has of its own.
    pub own: OwnToolServers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatToolServer {
    pub name: String,
    pub stands: Stands,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnToolServers {
    /// The agent says nothing of its servers, so they're never known.
    NeverSaid,
    /// The agent hasn't said yet, as before its first turn, and while it isn't running.
    NotKnown,
    /// By name, and none for a CLI with none.
    Listed(Vec<ChatToolServer>),
}

impl ChatToolServers {
    /// What `records` has to say of the servers of `session`'s agent, or `None` for nothing: where neither the hub nor the
    /// agent is known to have any. `sign_in` is the one this client has, if it has one, whichever chat it was started in.
    pub fn of(records: &Snapshot, session: &Session, sign_in: Option<&SignIn>) -> Option<Self> {
        let said = records.tool_servers_said(session);
        let servers = Self { hubs: hubs(records, said, sign_in), own: own(records, session, said) };
        let own = matches!(&servers.own, OwnToolServers::Listed(own) if !own.is_empty());
        (own || !servers.hubs.is_empty()).then_some(servers)
    }

    /// Whether one of the hub's needs the person to sign in to it, which it does until its record says otherwise.
    pub fn needs_sign_in(&self) -> bool {
        self.needing_sign_in().next().is_some()
    }

    /// What it all comes to in words, which is as much as the plug's colour says.
    pub fn said(&self) -> String {
        let names: Vec<&str> = self.needing_sign_in().map(|server| server.name.as_str()).collect();
        match names.as_slice() {
            [] => "MCP servers".to_string(),
            [one] => format!("MCP servers, {one} needs you to sign in"),
            several => format!("MCP servers, {} need you to sign in", several.join(", ")),
        }
    }

    fn needing_sign_in(&self) -> impl Iterator<Item = &ChatToolServer> {
        self.hubs.iter().filter(|server| matches!(server.stands, Stands::NeedsSignIn { .. }))
    }
}

fn hubs(records: &Snapshot, said: Option<&[ToolServerState]>, sign_in: Option<&SignIn>) -> Vec<ChatToolServer> {
    let said_of = |id| said.into_iter().flatten().find(|server| server.hubs.as_ref() == Some(id));
    let held = records.tool_servers().iter().map(|server| ChatToolServer {
        name: server.name.to_string(),
        stands: Stands::of_the_hubs(server, said_of(&server.id), said.is_some(), sign_in),
    });
    let gone = said.into_iter().flatten().filter_map(|server| {
        let id = server.hubs.as_ref()?;
        // The person's name for it went with its record.
        let removed = || ChatToolServer { name: server.name.to_string(), stands: Stands::Removed };
        records.tool_server(id).is_none().then(removed)
    });
    held.chain(by_name(gone)).collect()
}

fn own(records: &Snapshot, session: &Session, said: Option<&[ToolServerState]>) -> OwnToolServers {
    let Some(said) = said else {
        return if says(records, session) { OwnToolServers::NotKnown } else { OwnToolServers::NeverSaid };
    };
    let own = said.iter().filter(|server| server.hubs.is_none());
    let own = own.map(|server| ChatToolServer { name: server.name.to_string(), stands: Stands::of_its_own(server) });
    OwnToolServers::Listed(by_name(own))
}

/// Whether the agent says which servers it has, as its run claims, or as its kind does before a run has.
fn says(records: &Snapshot, session: &Session) -> bool {
    let claims = match &session.agent_capabilities {
        Some(run) => Some(run.claims.as_ref()),
        None => records.agents().of(session.agent).map(|agent| &agent.capabilities),
    };
    claims.is_some_and(|claims| claims.setup.tool_servers.is_some())
}

fn by_name(servers: impl Iterator<Item = ChatToolServer>) -> Vec<ChatToolServer> {
    let mut servers: Vec<ChatToolServer> = servers.collect();
    servers.sort_by_cached_key(|server| server.name.to_lowercase());
    servers
}
