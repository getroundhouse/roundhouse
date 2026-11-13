use crate::{Client, ClientError, Result};
use protocol::model::{AgentDescription, AgentKind};

/// Each kind of agent a hub runs, as it describes them, so a client offers what each takes. A hub too old to describe
/// its agents describes none, and a view then offers only what a chat already runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Agents {
    pub(super) described: Vec<AgentDescription>,
}

impl Agents {
    /// Asks the hub, which says the same for as long as it runs, so once a connection is enough. A hub too old to
    /// describe its agents describes none, and so does one whose answer can't be read, since what a client shows of a
    /// hub's records matters more. Only losing the connection fails.
    pub fn read(client: &Client) -> Result<Agents> {
        match client.agent_list() {
            Ok(described) => Ok(Agents { described }),
            Err(ClientError::Api(_) | ClientError::Unexpected(_)) => Ok(Agents::default()),
            Err(e @ (ClientError::Io(_) | ClientError::Disconnected)) => Err(e),
        }
    }

    /// `described`, as a hub would say them, for a test of what shows them.
    #[cfg(feature = "testing")]
    pub fn new(described: Vec<AgentDescription>) -> Agents {
        Agents { described }
    }

    /// Every kind's description, in the order the hub gave them.
    pub fn described(&self) -> &[AgentDescription] {
        &self.described
    }

    /// The description of each kind this build can name, in the order the hub gave them.
    pub fn named(&self) -> impl Iterator<Item = &AgentDescription> {
        self.described.iter().filter(|agent| agent.kind != AgentKind::Unknown)
    }

    /// The description of each kind this build can name whose login the hub can be given, as it says what makes one,
    /// in the order the hub gave them. None from a hub from before hubs kept logins, which says that of no kind.
    pub fn with_logins(&self) -> impl Iterator<Item = &AgentDescription> {
        self.named().filter(|agent| agent.login.is_some())
    }

    /// `kind`'s description. `None` for a kind the hub doesn't describe, one this build can't name, and before the hub
    /// has said.
    pub fn of(&self, kind: AgentKind) -> Option<&AgentDescription> {
        let named = kind != AgentKind::Unknown;
        self.described.iter().find(|agent| named && agent.kind == kind)
    }

    /// The kind a new chat runs: the first the hub describes that this build can name, or Claude, the only kind a hub
    /// too old to describe its agents runs.
    pub fn for_new_chat(&self) -> AgentKind {
        self.new_chat().map_or(AgentKind::Claude, |agent| agent.kind)
    }

    /// The description of what a new chat runs, as [`Agents::for_new_chat`] says. `None` from a hub too old to say.
    pub fn new_chat(&self) -> Option<&AgentDescription> {
        self.named().next()
    }
}
