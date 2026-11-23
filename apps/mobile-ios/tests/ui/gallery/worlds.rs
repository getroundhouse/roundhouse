//! What the hub holds for each of the phone's screens, built on [`World`]. The chats are the ones the desktop's
//! screens show, from `spec_screens`, so a chat reads the same in both galleries.

#![cfg(test)]

use client_core::fake_hub::World;
use protocol::model::{
    CheckState, Local, OwnPullRequest, PrState, SessionStatus, Summary, Title, WorkspaceId, WorkspaceStatus,
};
use protocol::pairing::{Fingerprint, PairingLink};
use spec_screens::{chats, proposal_chats, question_chats, subagent_chats};

/// The row of the workspace each screen about a chat opens: the fourth made, after the first repository's others.
pub(super) const FOCUS_ROW: &str = "workspace-ws-4";

/// A hub with repositories and nothing started in them.
pub(super) fn empty() -> World {
    let mut world = World::default();
    world.repo("roundhouse");
    world.repo("signal-box");
    world
}

/// Two repositories, with workspaces as a day's work leaves them: one merged, one at work, one waiting on an answer,
/// one whose agent stopped short, one with nothing said yet, and an archived one that stays hidden. Returns the
/// world, and the workspace the chats' screens open.
fn workspaces() -> (World, WorkspaceId) {
    let mut world = World::default();
    let roundhouse = world.repo("roundhouse");
    let signal_box = world.repo("signal-box");
    let made = |world: &mut World, repo, name: &str, title: Option<&str>, status| {
        let id = world.workspace(repo, name);
        world.workspace_mut(&id).title = title.and_then(Title::tidied);
        if let Some(status) = status {
            world.chat(&id, status, Vec::new());
        }
        id
    };
    let alder = made(&mut world, &roundhouse, "alder", Some("Fix the login redirect"), Some(SessionStatus::Idle));
    made(&mut world, &roundhouse, "birch", Some("Speed up opening a large diff"), Some(SessionStatus::Running));
    let cedar =
        made(&mut world, &roundhouse, "cedar", Some("Ask before deleting files"), Some(SessionStatus::WaitingApproval));
    let focus = made(&mut world, &roundhouse, "aspen", Some("Show PR state in the sidebar"), None);
    made(&mut world, &signal_box, "elm", Some("Retry a failed setup"), Some(SessionStatus::Interrupted));
    made(&mut world, &signal_box, "fir", None, Some(SessionStatus::Idle));
    let archived = made(&mut world, &signal_box, "hazel", None, None);
    world.workspace_mut(&archived).status = WorkspaceStatus::Archived;

    world.workspace_mut(&focus).branch = "sidebar-pr-state".into();
    let found = |change: fn(&mut Summary)| {
        let mut own = open();
        change(&mut own.pull_request.summary.said);
        own
    };
    world.github_says.pull_requests.insert(alder, found(|pr| pr.state = PrState::Merged));
    world.github_says.pull_requests.insert(cedar, found(|pr| pr.is_draft = true));
    world.github_says.pull_requests.insert(focus.clone(), found(|_| {}));
    (world, focus)
}

/// The workspaces, with a finished turn in the screens' own.
pub(super) fn hub() -> World {
    let (mut world, focus) = workspaces();
    let chat = world.chat(&focus, SessionStatus::Idle, chats::finished());
    world.last_turn_took(&chat, 108_000);
    world.read_reply(&chat);
    world
}

/// The workspaces, with the agent partway through a reply in the screens' own.
pub(super) fn working() -> World {
    let (mut world, focus) = workspaces();
    let chat = world.chat(&focus, SessionStatus::Running, chats::streaming());
    world.working_for(&chat, 12_000);
    world.tails.insert(chat, chats::streaming_tail());
    world
}

/// The workspaces, with the agent waiting on an answer to its question in the screens' own.
pub(super) fn asking() -> World {
    let (mut world, focus) = workspaces();
    world.chat(&focus, SessionStatus::WaitingApproval, question_chats::question());
    world
}

/// The workspaces, with the agent waiting for leave to run a command in the screens' own.
pub(super) fn asking_to_run() -> World {
    let (mut world, focus) = workspaces();
    world.chat(&focus, SessionStatus::WaitingApproval, chats::approval());
    world
}

/// The workspaces, with the agent's subagents' work done in the screens' own.
pub(super) fn subagents() -> World {
    let (mut world, focus) = workspaces();
    world.chat(&focus, SessionStatus::Idle, subagent_chats::subagents(false));
    world
}

/// The workspaces, with a proposal waiting on an answer in the screens' own.
pub(super) fn proposing() -> World {
    let (mut world, focus) = workspaces();
    let chat = world.chat(&focus, SessionStatus::Idle, proposal_chats::events());
    let session = world.sessions.iter().find(|session| session.id == chat).unwrap();
    let proposal = proposal_chats::proposal(session, vec![proposal_chats::first(None)]);
    world.proposals.insert(chat, vec![proposal]);
    world
}

/// The workspaces, with three chats in the screens' own: the one the screens open, another at work, and one closed an
/// hour ago.
pub(super) fn several_chats() -> World {
    let (mut world, focus) = workspaces();
    let working = world.chat(&focus, SessionStatus::Running, Vec::new());
    let closed = world.chat(&focus, SessionStatus::Idle, Vec::new());
    world.close(&closed, 60);
    let shown = world.chat(&focus, SessionStatus::Idle, chats::finished());
    world.last_turn_took(&shown, 108_000);
    world.read_reply(&shown);
    for (chat, title) in [
        (&shown, "Where would the sidebar's PR state come from?"),
        (&working, "Which colours does GitHub use for a PR's state?"),
        (&closed, "Why does the hub warn on every reconnect?"),
    ] {
        world.sessions.iter_mut().find(|session| &session.id == chat).unwrap().title = Title::tidied(title);
    }
    world
}

/// What another hub's QR code holds, as the Camera app opens it.
pub(super) fn link_to_another_hub() -> String {
    PairingLink {
        hub_name: "Studio".to_string(),
        hosts: vec!["studio.local:7421".to_string()],
        fingerprint: Fingerprint::of_sha256([2; 32]),
        code: "made-up-code".into(),
    }
    .to_string()
}

/// An open pull request with nothing in the way of merging it.
fn open() -> OwnPullRequest {
    let mut pull_request = protocol::fixtures::pull_request(182);
    let about = &mut pull_request.summary.said.about;
    about.url = "https://example.com/pull/182".to_string();
    about.title = "Show PR state in the sidebar".to_string();
    for check in &mut pull_request.checks.as_mut().unwrap().said.checks {
        check.state = CheckState::Passed;
    }
    OwnPullRequest { pull_request, local: Local::default() }
}
