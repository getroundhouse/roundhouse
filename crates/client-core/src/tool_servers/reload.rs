//! Having a chat's agent connect again to one of its tool servers: which rows of its card offer it, and how the ones this
//! client has asked for stand.

use std::collections::HashMap;

use protocol::capability::setup::ServerName;
use protocol::model::{Rev, Session, SessionId};

use super::Stands;
use crate::records::Snapshot;
use crate::refusals::couldnt;
use crate::{ClientError, Result};

/// What a client offers of having a server connected to again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reload {
    /// As the agent has it, which the hub is asked by.
    pub server: ServerName,
    pub offered: Offered,
    /// Whether this client has asked for it, and the hub hasn't said what came of that, so it isn't asked again.
    pub asked: bool,
}

/// How a reload is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offered {
    /// Beside how the server stands, until it's pointed at, since it works.
    Quietly,
    /// In the place of how it stands, which then goes under it, since it doesn't.
    Plainly,
}

/// The reloads this client has asked for on a connection to a hub, each until what the agent says of its servers is
/// newer than when it was asked for.
#[derive(Debug, Default)]
pub struct Reloads {
    asked: HashMap<(SessionId, ServerName), Asked>,
    /// The hub has said it doesn't know the request, so nothing offers it.
    unknown_to_the_hub: bool,
}

#[derive(Debug)]
struct Asked {
    /// The revision of what the agent said of its servers, as it was asked for.
    of: Option<Rev>,
    /// What to say of it, once the hub has refused it.
    refused: Option<String>,
}

impl Reloads {
    /// The hub is being asked to have `session`'s agent connect again to the server it has as `server`, as `records` says
    /// it is.
    pub fn ask(&mut self, records: &Snapshot, session: &SessionId, server: &ServerName) {
        let of = records.tool_servers_rev(session);
        // Those whose agent has said more are over.
        self.asked.retain(|(session, _), asked| records.tool_servers_rev(session) == asked.of);
        self.asked.insert((session.clone(), server.clone()), Asked { of, refused: None });
    }

    /// Takes in what the hub answered. One it took is still asked for, until the agent says what came of it.
    pub fn answered(&mut self, session: &SessionId, server: &ServerName, answer: &Result<()>) {
        let Err(e) = answer else { return };
        self.unknown_to_the_hub |= ClientError::is_unsupported(e);
        if let Some(asked) = self.asked.get_mut(&(session.clone(), server.clone())) {
            asked.refused = Some(couldnt("reload it", e));
        }
    }

    fn of(&self, records: &Snapshot, session: &SessionId, server: &ServerName) -> Option<&Asked> {
        let asked = self.asked.get(&(session.clone(), server.clone()))?;
        (records.tool_servers_rev(session) == asked.of).then_some(asked)
    }
}

/// What the card says of reloading a server.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct OfReload {
    pub(super) reload: Option<Reload>,
    /// Why the hub refused it, if it has.
    pub(super) refused: Option<String>,
}

impl OfReload {
    /// Of the server `session`'s agent has as `server`, which `stands` so.
    pub(super) fn of(
        records: &Snapshot,
        session: &Session,
        server: &ServerName,
        stands: &Stands,
        reloads: &Reloads,
    ) -> Self {
        let asked = reloads.of(records, &session.id, server);
        let reload = offered(session, stands).filter(|_| !reloads.unknown_to_the_hub).map(|offered| Reload {
            server: server.clone(),
            offered,
            asked: asked.is_some_and(|asked| asked.refused.is_none()),
        });
        Self { reload, refused: asked.and_then(|asked| asked.refused.clone()) }
    }
}

/// How a reload is offered of a server that `stands` so, if the run that's going claims it, and nothing else is there to
/// do about the server.
fn offered(session: &Session, stands: &Stands) -> Option<Offered> {
    let claims = &session.agent_capabilities.as_ref()?.claims.setup;
    match stands {
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
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::Claimed;
    use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
    use protocol::client::{ApiError, ErrorCode};
    use protocol::fixtures;
    use protocol::model::{RunCapabilities, RunId, SessionId};

    use super::super::tests::{held, held_as, named, needing_sign_in, said};
    use super::super::{ChatToolServer, ChatToolServers, OwnToolServers};
    use super::*;

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

    fn of(records: &Snapshot, session: &Session, reloads: &Reloads) -> Vec<ChatToolServer> {
        rows(ChatToolServers::of(records, session, None, reloads).unwrap())
    }

    /// The rows that offer it, by the agent's name for each server, and how.
    fn offering(session: &Session) -> Vec<(String, Offered)> {
        let offered = of(&one_of_each(), session, &Reloads::default()).into_iter().filter_map(|row| row.reload);
        offered.map(|reload| (reload.server.to_string(), reload.offered)).collect()
    }

    #[test]
    fn a_server_that_works_or_cant_be_used_is_offered_as_the_run_claims_and_one_with_nothing_to_reload_never_is() {
        let quietly = |name: &str| (name.to_string(), Offered::Quietly);
        let plainly = |name: &str| (name.to_string(), Offered::Plainly);

        let claude = offering(&chat(true, true));
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
        let codex = offering(&chat(true, false));
        assert_eq!(codex, claude.into_iter().filter(|(_, how)| *how == Offered::Plainly).collect::<Vec<_>>());
        assert_eq!(offering(&chat(false, false)), [], "a run of an older build, or an agent that can't");
        let none_said = Session { agent_capabilities: None, ..chat(true, true) };
        assert_eq!(offering(&none_said), [], "whatever its kind can do");
    }

    /// By what it takes of a run to connect to it as it stands. One the hub says it has from its next start isn't
    /// offered, whatever the run claims.
    #[test]
    fn one_the_agent_is_to_connect_to_once_its_turn_ends_is_offered_plainly_as_the_run_claims() {
        let offered = |when, state, session: &Session| {
            let said = ToolServerState { connects_again: Some(when), ..said("as-t-1", Some("t-1"), state) };
            let records = held(vec![named("t-1", "Docs")], Some(vec![said]));
            let [row] = &of(&records, session, &Reloads::default())[..] else { panic!("one server") };
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
        let of = |session| ChatToolServers::of(&one_of_each(), session, None, &Reloads::default()).unwrap();
        let only = |servers| {
            let records = held(Vec::new(), Some(servers));
            ChatToolServers::of(&records, &chat(true, false), None, &Reloads::default()).unwrap()
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

    /// What comes of it is known by its revision, since what's said can be the same as before, where it failed again before
    /// a client was told that it had started.
    #[test]
    fn one_asked_for_is_off_until_the_agent_has_said_more_of_its_servers() {
        let (docs, session, mut reloads) = (ServerName::from("docs"), chat(true, true), Reloads::default());
        let failed = |rev| held_as(Rev::new(rev), Vec::new(), Some(vec![said("docs", None, ServerState::Failed)]));
        let asked = |records: &Snapshot, reloads: &Reloads| {
            let [row] = &of(records, &session, reloads)[..] else { panic!("one server") };
            (row.reload.as_ref().unwrap().asked, row.why().map(str::to_owned))
        };
        assert_eq!(asked(&failed(1), &reloads), (false, None));

        reloads.ask(&failed(1), &session.id, &docs);
        assert_eq!(asked(&failed(1), &reloads), (true, None));
        reloads.answered(&session.id, &docs, &Ok(()));
        assert_eq!(asked(&failed(1), &reloads), (true, None), "its answer doesn't say what came of it");
        assert_eq!(asked(&failed(2), &reloads), (false, None));
    }

    #[test]
    fn a_refusal_is_said_until_its_asked_for_again_or_the_agent_has_said_more() {
        let (docs, session, mut reloads) = (ServerName::from("docs"), chat(true, true), Reloads::default());
        let servers = || {
            let failed = said("docs", None, ServerState::Failed);
            Some(vec![ToolServerState { error: Some("Connection refused".into()), ..failed }])
        };
        let (records, newer) =
            (held_as(Rev::new(1), Vec::new(), servers()), held_as(Rev::new(2), Vec::new(), servers()));
        let said_under = |records: &Snapshot, reloads: &Reloads| {
            let [row] = &of(records, &session, reloads)[..] else { panic!("one server") };
            (row.reload.as_ref().map(|reload| reload.asked), row.why().unwrap().to_owned())
        };
        let refuse = |reloads: &mut Reloads, code| {
            reloads.ask(&records, &session.id, &docs);
            let refused = Err(ClientError::Api(ApiError::new(code, "the chat's agent isn't running")));
            reloads.answered(&session.id, &docs, &refused);
        };

        refuse(&mut reloads, ErrorCode::Conflict);
        let refused = "Couldn't reload it: the chat's agent isn't running";
        assert_eq!(said_under(&records, &reloads), (Some(false), refused.into()), "and it can be asked for again");
        assert_eq!(said_under(&newer, &reloads), (Some(false), "Connection refused".into()));
        reloads.ask(&records, &session.id, &docs);
        assert_eq!(said_under(&records, &reloads), (Some(true), "Connection refused".into()));

        refuse(&mut reloads, ErrorCode::Unsupported);
        let unknown = "Couldn't reload it: this hub can't do that until it's updated";
        assert_eq!(said_under(&records, &reloads), (None, unknown.into()), "an older hub's, which nothing is asked of");
        assert_eq!(said_under(&newer, &reloads), (None, "Connection refused".into()), "for as long as it's the hub");
    }
}
