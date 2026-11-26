//! Having a chat's agent connect again to one of its tool servers: which rows of its card offer it.

use protocol::capability::setup::ServerName;
use protocol::model::Session;

use super::Stands;

/// What a client offers of having a server connected to again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reload {
    /// As the agent has it, which the hub is asked by.
    pub server: ServerName,
    pub offered: Offered,
}

/// How a reload is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offered {
    /// Beside how the server stands, until it's pointed at, since it works.
    Quietly,
    /// In the place of how it stands, which then goes under it, since it doesn't.
    Plainly,
}

/// Whether the hub knows the request to have a server connected to again, which a client finds out as the hub refuses
/// it, and keeps for as long as it's connected to that hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadRequest {
    Known,
    /// Nothing offers it.
    UnknownToTheHub,
}

impl Reload {
    /// What's offered of the server `session`'s agent has as `server`, which `stands` so, if the run that's going claims
    /// it, the hub knows the request, and nothing else is there to do about the server.
    pub(super) fn of(session: &Session, server: &ServerName, stands: &Stands, request: ReloadRequest) -> Option<Self> {
        let ReloadRequest::Known = request else { return None };
        let claims = &session.agent_capabilities.as_ref()?.claims.setup;
        let offered = match stands {
            Stands::Tools(_) | Stands::Connected => claims.tool_server_reload.map(|_| Offered::Quietly),
            Stands::WhenItsTurnEnds { connected: true } => claims.tool_server_reload.map(|_| Offered::Plainly),
            Stands::WhenItsTurnEnds { connected: false } | Stands::CantBeReached { .. } | Stands::SignInFromItsCli => {
                claims.tool_server_reconnect.map(|_| Offered::Plainly)
            }
            Stands::NotSaid
            | Stands::Starting
            | Stands::NeedsSignIn { .. }
            | Stands::TurnedOff
            | Stands::FromItsNextStart
            | Stands::Removed => None,
        };
        offered.map(|offered| Self { server: server.clone(), offered })
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::Claimed;
    use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
    use protocol::fixtures;
    use protocol::model::{RunCapabilities, RunId, SessionId};

    use super::super::tests::{held, named, needing_sign_in, said};
    use super::super::{ChatToolServer, ChatToolServers, OwnToolServers};
    use super::*;
    use crate::records::Snapshot;

    /// A chat whose agent's run says which servers it has, and claims it can connect to one it isn't connected to, and to
    /// one it is.
    fn chat(reconnects: bool, reloads: bool) -> Session {
        let mut claims = fixtures::claude_claims();
        claims.setup.tool_servers = Some(Claimed {});
        claims.setup.tool_server_reconnect = reconnects.then_some(Claimed {});
        claims.setup.tool_server_reload = reloads.then_some(Claimed {});
        let agent_capabilities = Some(RunCapabilities::new(RunId::new(1), claims));
        Session { id: SessionId::from("s"), agent_capabilities, ..fixtures::session("s") }
    }

    /// One of each way a server can stand: the hub's, and the CLI's own.
    fn one_of_each() -> Snapshot {
        let hubs = vec![
            named("t-1", "Counted"),
            named("t-2", "Connected"),
            named("t-3", "Failed"),
            named("t-4", "Agent says sign in"),
            named("t-5", "Starting"),
            named("t-6", "Off"),
            named("t-7", "Unknown"),
            named("t-8", "Not launched with"),
            needing_sign_in("t-9", "Needs sign in"),
        ];
        let of = |id: &str, state| said(&format!("as-{id}"), Some(id), state);
        let agents = vec![
            ToolServerState { tools: Some(3), ..of("t-1", ServerState::Connected) },
            of("t-2", ServerState::Connected),
            of("t-3", ServerState::Failed),
            of("t-4", ServerState::NeedsSignIn),
            of("t-5", ServerState::Starting),
            of("t-6", ServerState::Disabled),
            of("t-7", ServerState::Unknown),
            of("t-9", ServerState::Failed),
            of("t-99", ServerState::Failed),
            said("own connected", None, ServerState::Connected),
            said("own failed", None, ServerState::Failed),
            said("own sign in", None, ServerState::NeedsSignIn),
        ];
        held(hubs, Some(agents))
    }

    fn rows(servers: ChatToolServers) -> Vec<ChatToolServer> {
        let OwnToolServers::Listed(own) = servers.own else { panic!("{servers:?}") };
        servers.hubs.into_iter().chain(own).collect()
    }

    fn of(records: &Snapshot, session: &Session, request: ReloadRequest) -> Vec<ChatToolServer> {
        rows(ChatToolServers::of(records, session, None, request).unwrap())
    }

    /// The rows that offer it, by the agent's name for each server, and how.
    fn offering(session: &Session, request: ReloadRequest) -> Vec<(String, Offered)> {
        let offered = of(&one_of_each(), session, request).into_iter().filter_map(|row| row.reload);
        offered.map(|reload| (reload.server.to_string(), reload.offered)).collect()
    }

    #[test]
    fn a_server_that_works_or_cant_be_used_is_offered_as_the_run_claims_and_one_with_nothing_to_reload_never_is() {
        let quietly = |name: &str| (name.to_string(), Offered::Quietly);
        let plainly = |name: &str| (name.to_string(), Offered::Plainly);

        let known = |session: &Session| offering(session, ReloadRequest::Known);

        let claude = known(&chat(true, true));
        assert_eq!(
            claude,
            [
                plainly("as-t-4"),
                quietly("as-t-2"),
                quietly("as-t-1"),
                plainly("as-t-3"),
                quietly("own connected"),
                plainly("own failed"),
                plainly("own sign in"),
            ],
            "by the names the agent has them under, in the order they're listed"
        );
        let codex = known(&chat(true, false));
        assert_eq!(codex, claude.into_iter().filter(|(_, how)| *how == Offered::Plainly).collect::<Vec<_>>());
        assert_eq!(known(&chat(false, false)), [], "a run of an older build, or an agent that can't");
        let none_said = Session { agent_capabilities: None, ..chat(true, true) };
        assert_eq!(known(&none_said), [], "whatever its kind can do");
        assert_eq!(offering(&chat(true, true), ReloadRequest::UnknownToTheHub), [], "whatever the run claims");
    }

    /// By what it takes of a run to connect to it as it stands. One the hub says it has from its next start isn't
    /// offered, whatever the run claims.
    #[test]
    fn one_the_agent_is_to_connect_to_once_its_turn_ends_is_offered_plainly_as_the_run_claims() {
        let offered = |when, state, session: &Session| {
            let said = ToolServerState { connects_again: Some(when), ..said("as-t-1", Some("t-1"), state) };
            let records = held(vec![named("t-1", "Docs")], Some(vec![said]));
            let [row] = &of(&records, session, ReloadRequest::Known)[..] else { panic!("one server") };
            row.reload.as_ref().map(|reload| reload.offered)
        };
        let (claude, codex) = (chat(true, true), chat(true, false));
        let when_it_ends = |state, session| offered(ConnectsAgain::WhenItsTurnEnds, state, session);

        assert_eq!(when_it_ends(ServerState::Connected, &claude), Some(Offered::Plainly));
        assert_eq!(when_it_ends(ServerState::Failed, &claude), Some(Offered::Plainly));
        assert_eq!(when_it_ends(ServerState::Connected, &codex), None);
        assert_eq!(when_it_ends(ServerState::Failed, &codex), Some(Offered::Plainly));
        assert_eq!(offered(ConnectsAgain::AtItsNextStart, ServerState::Connected, &claude), None);
    }

    /// Which is what its card waits for the pointer for, and what's read out of it.
    #[test]
    fn a_card_says_whether_it_offers_anything_and_all_of_it_in_words() {
        let of = |session| ChatToolServers::of(&one_of_each(), session, None, ReloadRequest::Known).unwrap();
        let only = |servers| {
            let records = held(Vec::new(), Some(servers));
            ChatToolServers::of(&records, &chat(true, false), None, ReloadRequest::Known).unwrap()
        };

        assert!(of(&chat(false, false)).offers(), "one to sign in to");
        assert!(!only(vec![said("docs", None, ServerState::Connected)]).offers());
        let failed = only(vec![
            said("docs", None, ServerState::Connected),
            ToolServerState { error: Some("Refused".into()), ..said("search", None, ServerState::Failed) },
        ]);
        assert!(failed.offers());
        assert_eq!(failed.each_said(), "docs, Connected. search, Can't be reached, Refused.");
    }
}
