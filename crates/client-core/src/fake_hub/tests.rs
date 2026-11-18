//! That a client reads from the fake hub what its world holds, and is refused what would change it, and that where the
//! fake hub works something out of its world, it does so by the rule the hub follows, as `protocol` states it.

use protocol::client::{ErrorCode, log};
use protocol::event::{ContentBlock, EventBody, ExitReason, TaskKind};
use protocol::model::{AgentKind, Place, Seq, SessionStatus, SubagentId, SubagentStatus};

use super::events::{subagent_ended, subagent_started, taken, task, text, user_prompt, within};
use super::{World, serve};
use crate::{Client, ClientError};

/// A client of a fake hub serving `world`, and the directory its socket is in, which goes when the test ends.
fn served(world: World) -> (tempfile::TempDir, Client) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    serve(&socket, world);
    let client = Client::connect(&socket, "test").unwrap();
    (dir, client)
}

#[test]
fn a_client_reads_what_the_world_holds_and_is_refused_a_change() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let chat = world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let (_dir, client) = served(world);

    let records = client.records_list().unwrap();
    let added = client.repo_add("/src/birch");

    assert_eq!(records.repos.iter().map(|repo| repo.name.as_str()).collect::<Vec<_>>(), ["aspen"]);
    assert_eq!(records.workspaces.iter().map(|workspace| &workspace.id).collect::<Vec<_>>(), [&workspace]);
    assert_eq!(records.sessions.iter().map(|session| &session.id).collect::<Vec<_>>(), [&chat]);
    assert!(matches!(added, Err(ClientError::Api(refused)) if refused.code == ErrorCode::Unsupported));
    assert_eq!(client.records_list().unwrap().repos.len(), 1, "and nothing was added");
}

/// As `SessionStatus::counts_as_running` says: in a turn, or in the background on anything but waits.
#[test]
fn the_hub_s_status_counts_the_chats_running_as_the_hub_does() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let listed =
        |kind| vec![EventBody::AgentTasks { tasks: vec![task(kind, "toolu_1", "Watch")], stopped: Vec::new() }];
    world.chat(&workspace, SessionStatus::Running, Vec::new());
    world.chat(&workspace, SessionStatus::WaitingApproval, Vec::new());
    world.chat(&workspace, SessionStatus::Background, listed(TaskKind::Command));
    world.chat(&workspace, SessionStatus::Background, listed(TaskKind::Wait));
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    world.chat(&workspace, SessionStatus::Interrupted, Vec::new());
    let (_dir, client) = served(world);

    let status = client.hub_status().unwrap();

    assert_eq!(status.running_sessions, 3, "all but the one waiting on its waits, the idle one and the stopped one");
}

/// As `EventBody::sets_background` says: the newest list, until the agent exits and takes its work with it.
#[test]
fn a_chat_runs_in_the_background_what_its_agent_last_listed_until_it_exits() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let listed =
        EventBody::AgentTasks { tasks: vec![task(TaskKind::Command, "toolu_1", "Watch")], stopped: Vec::new() };
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] };
    let running = world.chat(&workspace, SessionStatus::Background, vec![listed.clone()]);
    let gone = world.chat(&workspace, SessionStatus::Interrupted, vec![listed, exited]);
    let (_dir, client) = served(world);

    let running = client.session_get(&running).unwrap();
    let gone = client.session_get(&gone).unwrap();

    assert_eq!(running.background_tasks.len(), 1);
    assert_eq!(gone.background_tasks, [], "its work went with it");
}

/// As `ReadLimit` says: at least one event, and none after the one that brings the read to its bytes.
#[test]
fn a_read_is_sent_as_much_as_a_hub_sends() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let chat = world.chat(&workspace, SessionStatus::Idle, vec![user_prompt("Fix it"), user_prompt("And the docs")]);
    let (_dir, client) = served(world);
    let read = |limit, max_bytes| {
        let read = log::SessionRead { id: chat.clone(), from_seq: Seq::new(1), limit, max_bytes, include_raw: false };
        client.call(read).unwrap().events.len()
    };

    assert_eq!(read(0, None), 1, "a read asking for none");
    assert_eq!(read(10, Some(1)), 1, "a read stopped by its bytes");
    assert_eq!(read(10, None), 2);
}

/// As `Session::is_open` says, so the newest chat that isn't closed.
#[test]
fn ensuring_a_workspace_has_a_chat_finds_its_newest_open_one() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let newest_open = world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let closed = world.chat(&workspace, SessionStatus::Idle, Vec::new());
    world.close(&closed, 5);
    let (_dir, client) = served(world);

    let ensured = client.session_ensure(&workspace, AgentKind::Claude).unwrap();

    assert_eq!(ensured.id, newest_open);
}

/// As `EventBody::subagent_started` says: by the ID its agent gives it, or by its call in one recorded before
/// subagents had IDs of their own. And as `Subagent::recorded_from` says, one that starts again after it ended works
/// again, and they're listed by when each last started.
#[test]
fn a_chat_s_subagents_go_by_the_id_their_agent_gives_or_by_their_call() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let named = |call: &str| {
        let mut named = subagent_started(call, "explore", "Look", "Look around");
        if let EventBody::SubagentStarted { subagent, .. } = &mut named {
            *subagent = Some("agent-7".into());
        }
        named
    };
    let events = vec![
        named("toolu_2"),
        subagent_started("toolu_1", "explore", "Read", "Read the docs"),
        subagent_ended("agent-7", "Looked.", 1_000),
        named("toolu_3"),
    ];
    let chat = world.chat(&workspace, SessionStatus::Running, events);
    let (_dir, client) = served(world);

    let (subagents, _) = client.session_subagents(&chat).unwrap();

    let ids: Vec<_> = subagents.iter().map(|subagent| (subagent.id(), subagent.status)).collect();
    let working = SubagentStatus::Working;
    assert_eq!(ids, [(SubagentId::of_call(&"toolu_1".into()), working), (SubagentId::from("agent-7"), working)]);
}

/// As `Lately::add` says: what a subagent wrote, and not what was written to it, cut short.
#[test]
fn what_a_subagent_has_done_lately_is_what_it_wrote_cut_short() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let told = EventBody::AgentUser {
        subagent: Some("toolu_1".into()),
        blocks: vec![ContentBlock::Text { text: "Look around".into(), images: Vec::new() }],
    };
    let long = "a".repeat(5_000);
    let events =
        vec![subagent_started("toolu_1", "explore", "Look", "Look around"), told, within("toolu_1", text("m1", &long))];
    let chat = world.chat(&workspace, SessionStatus::Running, events);
    let (_dir, client) = served(world);

    let (subagents, _) = client.session_subagents(&chat).unwrap();

    let since = subagents[0].lately.as_ref().and_then(|work| work.since.as_ref()).unwrap();
    assert!(since.cut && since.text.chars().all(|c| c == 'a'), "only what it wrote, cut short: {since:?}");
}

/// As `Subagent::named` says: a subagent its agent gave an ID is read, followed and searched by its call as by that ID.
#[test]
fn a_subagent_asked_for_by_its_call_is_the_one_its_call_started() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let mut named = subagent_started("toolu_2", "explore", "Look", "Look around");
    if let EventBody::SubagentStarted { subagent, .. } = &mut named {
        *subagent = Some("agent-7".into());
    }
    let chat = world.chat(&workspace, SessionStatus::Running, vec![named, within("agent-7", text("m1", "Found it"))]);
    let (_dir, client) = served(world);
    let by = |name: &str| {
        let name = SubagentId::from(name);
        let (read, last) = client.session_read_places(&chat, Some(&name), Place::new(1), 10).unwrap();
        let found = client.session_find(&chat, Some(&name), "Found").unwrap();
        (read.len(), last, found.places)
    };

    assert_eq!(by("toolu_2"), by("agent-7"));
    assert_eq!(by("agent-7"), (1, Place::new(1), vec![Place::new(1)]));
}

/// As `SettledAt::of` says: where the last event that took or dropped it is.
#[test]
fn a_chat_s_prompts_are_listed_with_where_each_was_settled() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let events = vec![user_prompt("Fix it"), taken(Seq::new(1)), user_prompt("And the docs")];
    let chat = world.chat(&workspace, SessionStatus::Running, events);
    let (_dir, client) = served(world);

    let prompts = client.session_prompt_list(&chat).unwrap();

    let settled: Vec<_> = prompts.iter().map(|line| line.settled().map(|at| at.seq.get())).collect();
    assert_eq!(settled, [Some(2), None]);
}
