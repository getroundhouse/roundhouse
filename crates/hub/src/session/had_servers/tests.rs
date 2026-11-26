//! Whether a run is asked to connect again to a server that's changed, by each thing that decides it.

use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
use protocol::capability::{Capabilities, Claimed};
use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::{RunId, ToolServerAuthEdit, ToolServerId, ToolServerKey};

use super::{HadServers, Running};
use crate::store::tests::open_in;
use crate::tool_servers::tests::{again, edit, named, signed_in, with_key};
use crate::tool_servers::{ServersAsSent, ToolServers};

const RUN: RunId = RunId::new(1);
const URL: &str = "https://mcp.example.com/mcp";
const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

/// What a hub has, and each way its one server is as it's changed.
struct Servers {
    servers: ToolServers,
    _dir: tempfile::TempDir,
}

impl Servers {
    /// With a key, which can't be told apart from a sign-in here.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let servers = ToolServers::new(open_in(dir.path()).into(), dir.path());
        servers.add(with_key("t", "Linear", "made-up")).unwrap();
        Self { servers, _dir: dir }
    }

    fn now(&self) -> ServersAsSent {
        self.servers.sent().borrow().as_sent().clone()
    }

    /// The servers as they are once `asked` of `t` is kept.
    fn after(&self, asked: ToolServerEdit) -> ServersAsSent {
        self.servers.edit(asked).unwrap();
        self.now()
    }

    fn another_key(&self) -> ServersAsSent {
        self.after(keyed("another", URL))
    }

    fn queried(&self) -> ServersAsSent {
        self.after(ToolServerEdit { url: Some(format!("{URL}?tools=some")), ..edit("t") })
    }

    fn moved(&self) -> ServersAsSent {
        self.after(ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed("another", ELSEWHERE) })
    }
}

fn keyed(key: &str, url: &str) -> ToolServerEdit {
    let key = Some(ToolServerKey::new(key));
    let auth = ToolServerAuthEdit::Key { header: "Authorization".into(), key, url: url.into() };
    ToolServerEdit { auth: Some(auth), ..edit("t") }
}

fn anyway(edit: ToolServerEdit) -> ToolServerEdit {
    ToolServerEdit { saved_anyway: true, ..edit }
}

fn claims(reconnects: bool, reloads: bool) -> Capabilities {
    let mut claims = Capabilities::default();
    claims.setup.tool_servers = Some(Claimed {});
    claims.setup.tool_server_reconnect = reconnects.then_some(Claimed {});
    claims.setup.tool_server_reload = reloads.then_some(Claimed {});
    claims
}

fn running(claims: Capabilities, on_a_turn: bool) -> Running {
    Running { claims, on_a_turn }
}

/// Claude's between turns.
fn idle() -> Running {
    running(claims(true, true), false)
}

fn says(state: ServerState) -> Vec<ToolServerState> {
    let (name, hubs) = ("Linear".into(), Some("t".into()));
    vec![ToolServerState { name, hubs, state, error: None, tools: None, connects_again: None }]
}

/// How many `running` is asked to connect to, as `had` it, and as it has them from now on, where each ask is delivered.
fn asks(had: &mut HadServers, now: &ServersAsSent, said: &[ToolServerState], running: &Running) -> usize {
    let due = undelivered(had, now, said, running);
    for (id, _) in &due {
        had.has_had(id, now.of(id).unwrap().clone());
    }
    due.len()
}

/// What it's asked to connect to, where no ask is delivered.
fn undelivered(
    had: &mut HadServers,
    now: &ServersAsSent,
    said: &[ToolServerState],
    running: &Running,
) -> Vec<(ToolServerId, ToolServerState)> {
    let looked = had.look(now, said, running);
    had.take(looked)
}

/// How many it's to connect to once its turn ends, whatever was said of that before.
fn waits(had: &HadServers, now: &ServersAsSent, said: &[ToolServerState], running: &Running) -> usize {
    let (mut said, looked) = (said.to_vec(), had.look(now, said, running));
    for said in &mut said {
        said.connects_again = Some(ConnectsAgain::Unknown);
    }
    looked.mark(&mut said);
    said.iter().filter(|said| said.connects_again == Some(ConnectsAgain::WhenItsTurnEnds)).count()
}

#[test]
fn a_run_is_asked_now_unless_its_on_a_turn() {
    for (on_a_turn, expected) in [(true, false), (false, true)] {
        assert_eq!(running(Capabilities::default(), on_a_turn).is_asked_now(), expected);
    }
}

/// Of a run that had it as it was added, and a server that's edited as `edit` says: whether it's asked once it's on no
/// turn, which is what it waits for while it's on one.
#[test]
fn what_changed_how_the_run_stands_and_what_it_claims_decide_whether_its_asked() {
    const CLAUDE: (bool, bool) = (true, true);
    const CODEX: (bool, bool) = (true, false);
    const ANTIGRAVITY: (bool, bool) = (false, false);
    type Edit = fn(&Servers) -> ServersAsSent;
    let renamed: Edit = |servers| servers.after(named("t", "Sentry"));
    let saved_anyway: Edit = |servers| servers.after(anyway(keyed("another", URL)));
    let moved_anyway: Edit = |servers| {
        let moving = ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed("another", ELSEWHERE) };
        servers.after(anyway(moving))
    };
    let edits: [(&str, Edit, [bool; 5]); 6] = [
        ("its name", renamed, [false; 5]),
        ("its key", Servers::another_key, [true, true, false, false, false]),
        ("its query", Servers::queried, [true, true, false, true, false]),
        ("where it is", Servers::moved, [true, true, false, true, false]),
        ("its key, anyway", saved_anyway, [true, true, false, true, false]),
        ("where it is, anyway", moved_anyway, [true, true, false, true, false]),
    ];
    let runs = [
        (ServerState::Failed, CLAUDE),
        (ServerState::NeedsSignIn, CODEX),
        (ServerState::Failed, ANTIGRAVITY),
        (ServerState::Connected, CLAUDE),
        (ServerState::Connected, CODEX),
    ];
    for (edited, edit, expected) in edits {
        for ((state, (reconnects, reloads)), expected) in runs.into_iter().zip(expected) {
            let case = format!("{edited}, {state:?}, reconnects: {reconnects}, reloads: {reloads}");
            let servers = Servers::new();
            let mut had = HadServers::new(RUN, servers.now());
            let said = says(state);
            let now = edit(&servers);
            let on_a_turn = running(claims(reconnects, reloads), true);
            let idle = running(claims(reconnects, reloads), false);
            let expected = usize::from(expected);

            assert_eq!(waits(&had, &now, &said, &on_a_turn), expected, "on a turn: {case}");
            assert_eq!(asks(&mut had, &now, &said, &on_a_turn), 0, "on a turn: {case}");
            assert_eq!(waits(&had, &now, &said, &idle), 0, "{case}");
            assert_eq!(asks(&mut had, &now, &said, &idle), expected, "{case}");
            assert_eq!(asks(&mut had, &now, &said, &idle), 0, "once: {case}");
            assert_eq!(waits(&had, &now, &said, &on_a_turn), 0, "once: {case}");
        }
    }
}

/// As a person's Reload is, so an ask that didn't reach the agent is asked for again, and once one has, not again.
#[test]
fn a_server_counts_as_had_only_once_its_ask_reaches_the_agent() {
    let servers = Servers::new();
    let mut had = HadServers::new(RUN, servers.now());
    let said = says(ServerState::Connected);
    let moved = servers.moved();

    assert_eq!(undelivered(&mut had, &moved, &said, &idle()).len(), 1);
    assert_eq!(undelivered(&mut had, &moved, &said, &idle()).len(), 1, "asked again");
    assert_eq!(asks(&mut had, &moved, &said, &idle()), 1);
    assert_eq!(asks(&mut had, &moved, &said, &idle()), 0, "once it's delivered");
}

#[test]
fn what_waits_for_a_turns_end_is_asked_once_then_whatever_else_changed_meanwhile() {
    let servers = Servers::new();
    let mut had = HadServers::new(RUN, servers.now());
    let said = says(ServerState::Connected);
    let on_a_turn = running(claims(true, true), true);

    assert_eq!(asks(&mut had, &servers.queried(), &said, &on_a_turn), 0);
    assert_eq!(asks(&mut had, &servers.another_key(), &said, &on_a_turn), 0);
    assert_eq!(asks(&mut had, &servers.now(), &said, &idle()), 1);
    assert_eq!(asks(&mut had, &servers.now(), &said, &idle()), 0);

    let queried_back = servers.after(ToolServerEdit { url: Some(URL.into()), ..edit("t") });
    assert_eq!(asks(&mut had, &queried_back, &said, &on_a_turn), 0);
    assert_eq!(asks(&mut had, &servers.queried(), &said, &idle()), 0, "it's as the run had it again");
}

/// Its calls have each already, so it's had, and one that fails afterwards failed with it.
#[test]
fn what_a_connected_run_has_by_its_calls_isnt_asked_for_once_it_fails() {
    let servers = Servers::new();
    let mut had = HadServers::new(RUN, servers.now());
    let codex = running(claims(true, false), false);

    for now in [servers.another_key(), servers.queried(), servers.moved()] {
        assert_eq!(asks(&mut had, &now, &says(ServerState::Connected), &codex), 0);
        assert_eq!(asks(&mut had, &now, &says(ServerState::Failed), &codex), 0);
    }
}

/// As of one carried on with from an earlier hub. A failed one is asked once more, and one that's connected has its
/// calls go as the server is.
#[test]
fn a_run_nothing_is_known_of_is_asked_for_a_failed_server_alone() {
    for (state, expected) in [(ServerState::Failed, 1), (ServerState::Connected, 0)] {
        let servers = Servers::new();
        let now = servers.moved();
        let mut had = HadServers::new(RUN, ServersAsSent::default());
        let said = says(state);

        assert_eq!(asks(&mut had, &now, &said, &idle()), expected, "{state:?}");
        assert_eq!(asks(&mut had, &now, &said, &idle()), 0, "once: {state:?}");
    }
}

#[test]
fn a_server_that_hasnt_settled_or_is_turned_off_waits_until_it_says_otherwise() {
    let servers = Servers::new();
    let mut had = HadServers::new(RUN, servers.now());
    let now = servers.queried();

    assert_eq!(asks(&mut had, &now, &[], &idle()), 0);
    for state in [ServerState::Unknown, ServerState::Starting, ServerState::Disabled] {
        assert_eq!(asks(&mut had, &now, &says(state), &idle()), 0, "{state:?}");
    }
    assert_eq!(asks(&mut had, &now, &says(ServerState::Failed), &idle()), 1, "once it says it failed");
}

#[test]
fn a_server_thats_the_clis_own_isnt_asked_for_whatever_its_called() {
    let servers = Servers::new();
    let mut had = HadServers::new(RUN, servers.now());
    let own = says(ServerState::Failed).into_iter().map(|said| ToolServerState { hubs: None, ..said });
    let own: Vec<_> = own.collect();

    assert_eq!(asks(&mut had, &servers.queried(), &own, &idle()), 0);
}

#[test]
fn a_failed_server_is_asked_for_once_for_each_sign_in_and_not_while_it_needs_one() {
    let dir = tempfile::tempdir().unwrap();
    let servers = ToolServers::new(open_in(dir.path()).into(), dir.path());
    let now = || servers.sent().borrow().as_sent().clone();
    let failed = says(ServerState::Failed);
    signed_in(&servers, "t", "Linear");
    let mut had = HadServers::new(RUN, now());

    assert_eq!(asks(&mut had, &now(), &failed, &idle()), 0, "as it launched, and as it's renewed");
    assert_eq!(asks(&mut had, &ServersAsSent::default(), &failed, &idle()), 0, "it's removed, or needs signing in to");
    again(&servers, "t").unwrap();
    assert_eq!(asks(&mut had, &now(), &failed, &idle()), 1);
    assert_eq!(asks(&mut had, &now(), &failed, &idle()), 0, "asked once");
}
