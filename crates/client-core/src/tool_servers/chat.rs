use protocol::capability::setup::ToolServerState;
use protocol::model::Session;

use super::reload::OfReload;
use super::{Reload, Reloads, SignIn, SignInFor, Stands};
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
    /// What a client offers of having its agent connect to it again, if it does.
    pub reload: Option<Reload>,
    /// Why the hub refused this client that, if it did.
    pub(super) reload_refused: Option<String>,
}

impl ChatToolServer {
    /// All that's said of it, as it's read out.
    pub fn said(&self) -> String {
        let words = self.stands.words();
        let said: Vec<&str> = [Some(self.name.as_str()), words.as_deref(), self.why()].into_iter().flatten().collect();
        said.join(", ")
    }

    /// Whether there's something to press for it: a sign-in, or a reload.
    fn offers(&self) -> bool {
        self.reload.is_some() || self.stands.to_sign_in_to().is_some()
    }

    /// What's said on the line under it: why the hub refused to have it connected to again, or what [`Stands::why`] says.
    pub fn why(&self) -> Option<&str> {
        self.reload_refused.as_deref().or_else(|| self.stands.why())
    }
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
    /// agent is known to have any. `sign_in` is the one this client has, if it has one, whichever chat it was started in,
    /// and `reloads` those it has asked for.
    pub fn of(records: &Snapshot, session: &Session, sign_in: Option<&SignIn>, reloads: &Reloads) -> Option<Self> {
        let said = records.tool_servers_said(session);
        let row = |name: String, agents: Option<&ToolServerState>, stands: Stands| {
            let agents = agents.map(|said| OfReload::of(records, session, &said.name, &stands, reloads));
            let OfReload { reload, refused } = agents.unwrap_or_default();
            ChatToolServer { name, stands, reload, reload_refused: refused }
        };
        let servers = Self { hubs: hubs(records, said, sign_in, &row), own: own(records, session, said, &row) };
        let own = matches!(&servers.own, OwnToolServers::Listed(own) if !own.is_empty());
        (own || !servers.hubs.is_empty()).then_some(servers)
    }

    /// Whether one of the hub's needs the person to sign in to it, to be used or to be used for more, which it does until
    /// its record says otherwise.
    pub fn needs_sign_in(&self) -> bool {
        self.hubs.iter().any(|server| server.stands.sign_in_for().is_some())
    }

    /// Whether there's something to press for any of them.
    pub fn offers(&self) -> bool {
        self.each().any(ChatToolServer::offers)
    }

    /// What's said of every one of them, as it's read out.
    pub fn each_said(&self) -> String {
        self.each().map(|server| format!("{}.", server.said())).collect::<Vec<_>>().join(" ")
    }

    /// The hub's, then its CLI's own.
    fn each(&self) -> impl Iterator<Item = &ChatToolServer> {
        let own = match &self.own {
            OwnToolServers::Listed(own) => own.as_slice(),
            OwnToolServers::NeverSaid | OwnToolServers::NotKnown => &[],
        };
        self.hubs.iter().chain(own)
    }

    /// What it all comes to in words, which is as much as the plug's colour says.
    pub fn said(&self) -> String {
        let of = |wanted, one_does, several_do| {
            let needing = self.hubs.iter().filter(|server| server.stands.sign_in_for() == Some(wanted));
            let names: Vec<&str> = needing.map(|server| server.name.as_str()).collect();
            match names.as_slice() {
                [] => None,
                [one] => Some(format!("{one} {one_does}")),
                several => Some(format!("{} {several_do}", several.join(", "))),
            }
        };
        let needing = of(SignInFor::Use, "needs you to sign in", "need you to sign in");
        let wanting = of(SignInFor::MoreAccess, "wants more access", "want more access");
        let said: Vec<String> = [Some("MCP servers".to_string()), needing, wanting].into_iter().flatten().collect();
        said.join(", ")
    }
}

/// Makes a row, of what it's called, how the agent says it is, if it does, and how it stands.
type Row<'a> = &'a dyn Fn(String, Option<&ToolServerState>, Stands) -> ChatToolServer;

fn hubs(
    records: &Snapshot,
    said: Option<&[ToolServerState]>,
    sign_in: Option<&SignIn>,
    row: Row,
) -> Vec<ChatToolServer> {
    let said_of = |id| said.into_iter().flatten().find(|server| server.hubs.as_ref() == Some(id));
    let held = records.tool_servers().iter().map(|server| {
        let agents = said_of(&server.id);
        row(server.name.to_string(), agents, Stands::of_the_hubs(server, agents, said.is_some(), sign_in))
    });
    let gone = said.into_iter().flatten().filter_map(|server| {
        let id = server.hubs.as_ref()?;
        // The person's name for it went with its record.
        let removed = || row(server.name.to_string(), Some(server), Stands::Removed);
        records.tool_server(id).is_none().then(removed)
    });
    held.chain(by_name(gone)).collect()
}

fn own(records: &Snapshot, session: &Session, said: Option<&[ToolServerState]>, row: Row) -> OwnToolServers {
    let Some(said) = said else {
        return if says(records, session) { OwnToolServers::NotKnown } else { OwnToolServers::NeverSaid };
    };
    let own = said.iter().filter(|server| server.hubs.is_none());
    let own = own.map(|server| row(server.name.to_string(), Some(server), Stands::of_its_own(server)));
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
