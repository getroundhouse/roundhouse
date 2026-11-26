//! What a run has had, what it's due by that, and when.

use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
use protocol::capability::{Capabilities, Control};
use protocol::model::{RunId, ToolServerId};

use crate::tool_servers::{ServerAsSent, ServersAsSent};

/// How a run has had each of its tool servers: as its worker had them as it launched, and as each was since when the run
/// was asked to connect to it again, whoever asked, or had it as it is without that. What isn't here nothing says, as of
/// every server of a run carried on with from an earlier hub.
pub(in super::super) struct HadServers {
    run: RunId,
    had: ServersAsSent,
}

/// The run that's going, as far as it decides what it's due, and when.
pub(in super::super) struct Running {
    pub(in super::super) claims: Capabilities,
    pub(in super::super) on_a_turn: bool,
}

impl Running {
    /// Whether it's asked now for what it's due, whatever that is, which is decided here alone. Connecting can end its
    /// calls to a server it's connected to, and it takes nothing the person sends it meanwhile, so a turn isn't cut into.
    pub(super) fn is_asked_now(&self) -> bool {
        !self.on_a_turn
    }
}

/// How a run stands with the servers it says it has, in the order it says them.
#[derive(Default)]
pub(in super::super) struct Looked {
    /// Those it hasn't had as they are, and has, or is to connect to now.
    had: Vec<Had>,
    /// Those it's to connect to later, and when, which is what clients are told of each.
    waiting: Vec<(ToolServerId, ConnectsAgain)>,
}

struct Had {
    id: ToolServerId,
    now: ServerAsSent,
    /// What it's to connect to for that, as it says it is.
    by_connecting: Option<ToolServerState>,
}

impl Looked {
    /// Whether there's anything to do, where the session's clients are told of `said`.
    pub(in super::super) fn finds_anything(&self, said: &[ToolServerState]) -> bool {
        !self.had.is_empty() || said.iter().any(|said| said.connects_again != self.waits(said))
    }

    /// `said`, with when the run is to connect again to each it hasn't had as it is, and isn't to connect to now.
    pub(in super::super) fn mark(&self, said: &mut [ToolServerState]) {
        for said in said {
            said.connects_again = self.waits(said);
        }
    }

    fn waits(&self, said: &ToolServerState) -> Option<ConnectsAgain> {
        let id = said.hubs.as_ref()?;
        self.waiting.iter().find(|(waiting, _)| waiting == id).map(|(_, when)| *when)
    }
}

impl HadServers {
    pub(in super::super) fn new(run: RunId, had: ServersAsSent) -> Self {
        Self { run, had }
    }

    pub(in super::super) fn is_of(&self, run: RunId) -> bool {
        self.run == run
    }

    /// How the run stands with `said`, the servers it says it has, as they are `now`.
    pub(in super::super) fn look(&self, now: &ServersAsSent, said: &[ToolServerState], running: &Running) -> Looked {
        let mut looked = Looked::default();
        for said in said {
            let Some(id) = &said.hubs else { continue };
            let Some(now) = now.of(id) else { continue };
            let had = self.had.of(id);
            if had == Some(now) {
                continue;
            }
            let has = |by_connecting| Had { id: id.clone(), now: now.clone(), by_connecting };
            match due(now, had, said, &running.claims) {
                Due::Nothing => looked.had.push(has(None)),
                Due::Connecting if running.is_asked_now() => looked.had.push(has(Some(said.clone()))),
                Due::Connecting => looked.waiting.push((id.clone(), ConnectsAgain::WhenItsTurnEnds)),
                Due::CutOff => looked.waiting.push((id.clone(), ConnectsAgain::AtItsNextStart)),
                Due::NotKnown => {}
            }
        }
        looked
    }

    /// Takes what it was found to be due, to ask for. What it has already counts as had from now on. What it's to connect
    /// to counts as had only once the ask reaches the agent, as [`Self::has_had`] says, whatever connecting comes to, so
    /// each is asked for until an ask does.
    pub(in super::super) fn take(&mut self, looked: Looked) -> Vec<(ToolServerId, ToolServerState)> {
        let mut due = Vec::new();
        for Had { id, now, by_connecting } in looked.had {
            match by_connecting {
                Some(server) => due.push((id, server)),
                None => self.had.set(id, now),
            }
        }
        due
    }

    /// It has had the server `id` as it is `now`, as once an ask to connect to it again has reached the agent.
    pub(in super::super) fn has_had(&mut self, id: &ToolServerId, now: ServerAsSent) {
        self.had.set(id.clone(), now);
    }
}

/// What a run is due of a server it hasn't had as it is now.
enum Due {
    /// It has it.
    Nothing,
    Connecting,
    /// It's connected to it where it was before it was moved, so its worker has cut it off from it, and it can't be asked
    /// to connect to it again, so it has it from its next start.
    CutOff,
    /// Until the run says otherwise how it stands, such as once it has failed to connect, or the person turns it on. One it
    /// can't be asked to connect to stays so as long as it runs, and its next start has the server as it is.
    NotKnown,
}

/// What the run that had a server as `had`, if anything says how, and says it's as `said`, is due of it, as it is `now`.
fn due(now: &ServerAsSent, had: Option<&ServerAsSent>, said: &ToolServerState, claims: &Capabilities) -> Due {
    match said.state {
        ServerState::Failed | ServerState::NeedsSignIn => {
            let reconnects = Control::Reconnect { server: said.name.clone() }.allowed(claims);
            if reconnects { Due::Connecting } else { Due::NotKnown }
        }
        ServerState::Connected => {
            let reloads = Control::Reload { server: said.name.clone() }.allowed(claims);
            match (changed_while_connected(now, had, said), reloads) {
                (Changed::Place | Changed::Tools, true) => Due::Connecting,
                (Changed::Place, false) => Due::CutOff,
                // Its calls go as the server is.
                (Changed::Tools, false) | (Changed::KnownBy, _) => Due::Nothing,
            }
        }
        ServerState::Starting | ServerState::Disabled | ServerState::Unknown => Due::NotKnown,
    }
}

/// What has changed most of a server a run is connected to, since the run had it.
enum Changed {
    Place,
    /// The tools it lists may be others, which connecting again reads.
    Tools,
    /// Nothing that its calls don't have already, which go as its worker has the server.
    KnownBy,
}

/// Where nothing says how the run had it, only where its worker says it connected does, so what else changed as no hub
/// ran is taken as had. Where that's where it is now, the run has listed its tools there, whatever it had before.
fn changed_while_connected(now: &ServerAsSent, had: Option<&ServerAsSent>, said: &ToolServerState) -> Changed {
    let place = now.place();
    let had_there = had.filter(|had| had.place() == place);
    let moved = match &said.connected_to {
        Some(connected_to) => connected_to != place,
        None => had.is_some() && had_there.is_none(),
    };
    if moved {
        Changed::Place
    } else if had_there.is_some_and(|had| now.lists_other_tools_than(had)) {
        Changed::Tools
    } else {
        Changed::KnownBy
    }
}
