use protocol::capability::Claimed;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::client::listed::SessionRecords;
use protocol::fixtures::{self, observed};
use protocol::model::{
    Observation, Records, Rev, RunCapabilities, RunId, Session, SessionId, ToolServer, ToolServerAuth, ToolServerName,
    ToolServerUrl, ToolSignInStatus,
};

use super::{
    ChatToolServer, ChatToolServers, OwnToolServers, Reloads, SignIn, SignInFor, SignInStands, Stands, Tone,
    ToolServerNames,
};
use crate::records::{HubRecords, Snapshot};

pub(super) fn named(id: &str, name: &str) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..fixtures::tool_server(id) }
}

pub(super) fn needing_sign_in(id: &str, name: &str) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..fixtures::tool_server_needing_sign_in(id) }
}

pub(super) fn said(name: &str, hubs: Option<&str>, state: ServerState) -> ToolServerState {
    ToolServerState {
        name: name.into(),
        hubs: hubs.map(Into::into),
        connected_to: None,
        state,
        error: None,
        tools: None,
    }
}

/// What a client holds of a hub with `servers`, whose chat's agent has said it has `said`, if it's said.
pub(super) fn held(servers: Vec<ToolServer>, said: Option<Vec<ToolServerState>>) -> Snapshot {
    held_as(Rev::new(1), servers, said)
}

/// The same, where the agent said it at `rev`.
pub(super) fn held_as(rev: Rev, servers: Vec<ToolServer>, said: Option<Vec<ToolServerState>>) -> Snapshot {
    let as_said = |said| {
        let mut observation = observed::tool_servers("s", said);
        if let Observation::SessionToolServers { rev: of, .. } = &mut observation {
            *of = rev;
        }
        observation
    };
    let observations = said.map(as_said).into_iter().collect();
    let mut held = HubRecords::default();
    held.apply_list(Records { tool_servers: servers, ..Records::default() });
    held.want_session(&"s".into());
    held.apply_session_list(SessionRecords {
        session_id: "s".into(),
        subagents: Vec::new(),
        proposals: Vec::new(),
        replies_heard: Vec::new(),
        observations,
        rev,
    });
    held.snapshot()
}

/// The chat, whose agent's run claims to say which servers it has, or not to.
fn chat(says: bool) -> Session {
    let mut claims = fixtures::claude_claims();
    claims.setup.tool_servers = says.then_some(Claimed {});
    let agent_capabilities = Some(RunCapabilities::new(RunId::new(1), claims));
    Session { id: SessionId::from("s"), agent_capabilities, ..fixtures::session("s") }
}

fn listed(servers: &[ChatToolServer]) -> Vec<(&str, Stands)> {
    servers.iter().map(|server| (server.name.as_str(), server.stands.clone())).collect()
}

#[test]
fn a_chat_has_the_hubs_servers_by_the_persons_names_and_its_clis_by_theirs() {
    let hub = vec![
        named("t-1", "runbooks"),
        named("t-2", "Linear (Northwind)"),
        named("t-3", "Docs"),
        needing_sign_in("t-4", "Notion"),
        named("t-5", "Mail"),
    ];
    let agents = vec![
        said("zebra", None, ServerState::NeedsSignIn),
        ToolServerState { tools: Some(3), ..said("Browser", None, ServerState::Connected) },
        ToolServerState { tools: Some(1), ..said("Linear_Northwind", Some("t-2"), ServerState::Connected) },
        ToolServerState { error: Some("refused".into()), ..said("runbooks", Some("t-1"), ServerState::Failed) },
        said("Notes", Some("t-4"), ServerState::Connected),
        said("Mail", Some("t-5"), ServerState::Unknown),
        said("Old_one", Some("t-9"), ServerState::Connected),
    ];

    let servers = ChatToolServers::of(&held(hub, Some(agents)), &chat(true), None, &Reloads::default()).unwrap();

    assert_eq!(
        listed(&servers.hubs),
        [
            ("Docs", Stands::FromItsNextStart),
            ("Linear (Northwind)", Stands::Tools(1)),
            ("Mail", Stands::NotSaid),
            ("Notion", Stands::NeedsSignIn { server: "t-4".into(), wanted: SignInFor::Use, sign_in: None }),
            ("runbooks", Stands::CantBeReached { why: Some("refused".into()) }),
            ("Old_one", Stands::Removed),
        ],
        "by which they are, so one renamed since is itself, and a sign-in is the record's to say"
    );
    let OwnToolServers::Listed(own) = &servers.own else { panic!("{servers:?}") };
    assert_eq!(listed(own), [("Browser", Stands::Tools(3)), ("zebra", Stands::SignInFromItsCli)]);
    assert!(servers.needs_sign_in());
    assert_eq!(servers.said(), "MCP servers, Notion needs you to sign in");
}

/// By where its worker says the agent last connected to it, however the agent says it is, and whatever its query is.
#[test]
fn one_thats_moved_since_the_agent_connected_to_it_is_from_its_next_start() {
    let url = |url: &str| ToolServerUrl::stored(url.to_owned());
    let server = |at: &str| ToolServer { url: url(at), ..named("t-1", "Docs") };
    let stands = |at: &str, connected_to: Option<&str>, state| {
        let connected_to = connected_to.map(|at| url(at).place());
        let agents = vec![ToolServerState { connected_to, tools: Some(2), ..said("Docs", Some("t-1"), state) }];
        let servers =
            ChatToolServers::of(&held(vec![server(at)], Some(agents)), &chat(true), None, &Reloads::default()).unwrap();
        servers.hubs[0].stands.clone()
    };

    for state in [ServerState::Connected, ServerState::Failed] {
        let moved = stands("https://eu.example.com/mcp", Some("https://mcp.example.com/mcp"), state);
        assert_eq!(moved, Stands::FromItsNextStart, "{state:?}");
    }
    let queried =
        stands("https://mcp.example.com/mcp?a=b", Some("https://mcp.example.com/mcp"), ServerState::Connected);
    assert_eq!(queried, Stands::Tools(2));
    assert_eq!(stands("https://eu.example.com/mcp", None, ServerState::Connected), Stands::Tools(2), "it hasn't said");
}

#[test]
fn one_of_the_hubs_that_only_the_agent_says_to_sign_in_to_cant_be_reached_and_one_of_the_clis_never_needs_you() {
    let agents = vec![said("Docs", Some("t-1"), ServerState::NeedsSignIn), said("own", None, ServerState::NeedsSignIn)];

    let servers =
        ChatToolServers::of(&held(vec![named("t-1", "Docs")], Some(agents)), &chat(true), None, &Reloads::default())
            .unwrap();

    assert_eq!(listed(&servers.hubs), [("Docs", Stands::CantBeReached { why: None })]);
    assert!(!servers.needs_sign_in());
    assert_eq!(servers.said(), "MCP servers");
}

/// It works, so the plug's words and its own say what it wants, and whatever its agent says of it waits.
#[test]
fn one_that_wants_more_access_is_signed_in_to_again_for_it() {
    let wanting = |id, name: &str| ToolServer {
        name: ToolServerName::stored(name.into()),
        ..fixtures::tool_server_wanting_more_access(id)
    };
    let hub = vec![wanting("t-1", "Linear"), needing_sign_in("t-2", "Notion"), wanting("t-3", "Docs")];
    let agents = vec![ToolServerState { tools: Some(14), ..said("Linear", Some("t-1"), ServerState::Connected) }];
    let held = held(hub, Some(agents));
    let linear = |stands: Option<SignInStands>| {
        let sign_in = stands.map(|stands| SignIn { to: "t-1".into(), stands });
        let servers = ChatToolServers::of(&held, &chat(true), sign_in.as_ref(), &Reloads::default()).unwrap();
        assert!(servers.needs_sign_in());
        assert_eq!(servers.said(), "MCP servers, Notion needs you to sign in, Docs, Linear want more access");
        let stands = &servers.hubs[1].stands;
        (stands.words(), stands.why().map(str::to_owned), stands.to_sign_in_to().is_some())
    };
    let again = || Some("Needs you to sign in again".to_owned());

    assert_eq!(linear(None), (again(), Some("Wants more access".into()), true));
    assert_eq!(linear(Some(SignInStands::UnderWay)), (Some("Signing in".into()), None, false));
    assert_eq!(linear(Some(came_to_nothing())), (again(), Some("It was refused".into()), true));
}

#[test]
fn only_a_sign_in_thats_gone_or_wants_more_access_is_the_persons_to_do() {
    let wanted = |status| SignInFor::of(&ToolServerAuth::SignIn { status });

    assert_eq!(wanted(ToolSignInStatus::NeedsSignIn), Some(SignInFor::Use));
    assert_eq!(wanted(ToolSignInStatus::NeedsMoreAccess), Some(SignInFor::MoreAccess));
    assert_eq!(wanted(ToolSignInStatus::Works), None);
    assert_eq!(wanted(ToolSignInStatus::Unknown), None, "a newer hub's");
    assert_eq!([SignInFor::of(&ToolServerAuth::Neither), SignInFor::of(&ToolServerAuth::Unknown)], [None, None]);
}

fn came_to_nothing() -> SignInStands {
    SignInStands::CameToNothing("It was refused".into())
}

/// A client offers it again once it has come to nothing, and the plug goes by the record all along.
#[test]
fn a_sign_in_is_said_of_the_server_its_to_while_the_hubs_record_says_it_needs_one() {
    let hub =
        held(vec![needing_sign_in("t-1", "Linear"), needing_sign_in("t-2", "Notion"), named("t-3", "Docs")], None);
    let of = |to: &str, stands| {
        let servers =
            ChatToolServers::of(&hub, &chat(true), Some(&SignIn { to: to.into(), stands }), &Reloads::default())
                .unwrap();
        assert_eq!(servers.said(), "MCP servers, Linear, Notion need you to sign in");
        let said = |server: &ChatToolServer| {
            let offered = server.stands.to_sign_in_to().is_some();
            (server.stands.words(), server.stands.why().map(str::to_owned), offered)
        };
        servers.hubs.iter().map(said).collect::<Vec<_>>()
    };
    let needs = || (Some("Needs you to sign in".to_owned()), None, true);
    let nothing = || (None, None, false);

    assert_eq!(
        of("t-2", SignInStands::UnderWay),
        [nothing(), needs(), (Some("Signing in".into()), None, false)],
        "in the order of their names"
    );
    assert_eq!(of("t-2", came_to_nothing()), [nothing(), needs(), (needs().0, Some("It was refused".into()), true)]);
    assert_eq!(of("t-3", SignInStands::UnderWay), [nothing(), needs(), needs()], "one that works");
    assert_eq!(of("t-9", came_to_nothing()), [nothing(), needs(), needs()]);
}

/// Not known, rather than none, until the agent has said, and of an agent that never says.
#[test]
fn a_chat_whose_agent_hasnt_said_has_the_hubs_with_nothing_said_of_them() {
    let unsaid = |says| {
        ChatToolServers::of(&held(vec![named("t-1", "Docs")], None), &chat(says), None, &Reloads::default()).unwrap()
    };

    assert_eq!(listed(&unsaid(true).hubs), [("Docs", Stands::NotSaid)]);
    assert_eq!(unsaid(true).own, OwnToolServers::NotKnown);
    assert_eq!(listed(&unsaid(false).hubs), [("Docs", Stands::NotSaid)]);
    assert_eq!(unsaid(false).own, OwnToolServers::NeverSaid);

    let nothing = |said| ChatToolServers::of(&held(Vec::new(), said), &chat(true), None, &Reloads::default());
    assert_eq!(nothing(None), None);
    assert_eq!(nothing(Some(Vec::new())), None);
    let own = nothing(Some(vec![said("own", None, ServerState::Starting)])).unwrap();
    assert_eq!((own.hubs, own.own != OwnToolServers::NotKnown), (Vec::new(), true));
}

#[test]
fn each_state_is_said_in_a_few_words_and_only_whats_in_the_way_is_a_problem() {
    let to_sign_in_to = |sign_in| Stands::NeedsSignIn { server: "t-1".into(), wanted: SignInFor::Use, sign_in };
    let of = |state, tools| Stands::of_its_own(&ToolServerState { tools, ..said("own", None, state) });
    let cases = [
        (of(ServerState::Connected, Some(1)), Some("1 tool"), Tone::Plain),
        (of(ServerState::Connected, Some(0)), Some("0 tools"), Tone::Plain),
        (of(ServerState::Connected, None), Some("Connected"), Tone::Plain),
        (of(ServerState::Starting, None), Some("Starting"), Tone::Plain),
        (of(ServerState::NeedsSignIn, None), Some("Sign in from its CLI"), Tone::Problem),
        (of(ServerState::Failed, None), Some("Can't be reached"), Tone::Problem),
        (of(ServerState::Disabled, None), Some("Turned off"), Tone::Plain),
        (of(ServerState::Unknown, None), None, Tone::Plain),
        (to_sign_in_to(None), Some("Needs you to sign in"), Tone::Problem),
        (to_sign_in_to(Some(SignInStands::UnderWay)), Some("Signing in"), Tone::Coming),
        (to_sign_in_to(Some(came_to_nothing())), Some("Needs you to sign in"), Tone::Problem),
        (Stands::FromItsNextStart, Some("From its next start"), Tone::Coming),
        (Stands::Removed, Some("Removed"), Tone::Plain),
    ];

    for (stands, words, tone) in cases {
        assert_eq!((stands.words().as_deref(), stands.tone()), (words, tone), "{stands:?}");
    }
}

#[test]
fn a_server_is_shown_by_the_persons_name_for_it_as_the_hub_has_it_now() {
    let names = ToolServerNames::of(&[named("t-1", "Linear (Northwind)"), named("t-2", "Docs")]);
    let shown = |server: &str| names.shown(&server.into()).to_string();

    assert_eq!(shown("Linear_Northwind"), "Linear (Northwind)");
    assert_eq!(shown("Docs"), "Docs");
    assert_eq!(shown("linear_northwind"), "linear_northwind", "another, since agents are given its case");
    assert_eq!(shown("github"), "github");
}
