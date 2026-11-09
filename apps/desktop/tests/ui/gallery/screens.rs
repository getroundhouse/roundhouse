//! The screens the gallery draws: what the hub holds for each, and what to do in the app before taking it.

#![cfg(test)]

use protocol::model::{
    CommentId, DiffComment, DiffSide, GitDiff, PullRequest, Rev, SessionStatus, SetupStatus, WorkspaceId,
    WorkspaceStatus,
};

use super::chats;
use super::world::{HubBuild, World};

pub(crate) struct Screen {
    /// Also the image's file name.
    pub(crate) name: &'static str,
    /// The spec's file for the area it's about, in `docs/spec/`, which shows it beside the rules it goes with.
    pub(crate) spec: &'static str,
    /// What the hub holds. `None` is no hub at all.
    pub(crate) world: Option<fn() -> World>,
    pub(crate) steps: &'static [Step],
}

/// Something to do in the app once it has connected, each done once the one before it has finished.
pub(crate) enum Step {
    /// Selects the workspace with this name, and waits for its chat to load.
    Select(&'static str),
    OpenDiff,
    OpenSetupLog,
    /// Presses a button by its ID, such as one that opens a menu.
    Click(&'static str),
    /// Types this where focus is, such as the composer.
    Type(&'static str),
    /// Presses Finish now in the update's card, and waits for what it asks.
    AskToFinish,
    /// Has the app show this version as downloaded, as if an update had come.
    UpdateReady(&'static str),
    /// Has the app show that it couldn't reach the update server.
    UpdateProblem,
}

/// The workspace each screen is about, in the first repository, below the ones the sidebar always has.
const FOCUS: &str = "aspen";

pub(crate) const SCREENS: &[Screen] = &[
    Screen { name: "no-hub", spec: "updates.md", world: None, steps: &[] },
    Screen { name: "first-run", spec: "workspaces.md", world: Some(World::default), steps: &[] },
    Screen {
        name: "setup-failed",
        spec: "workspaces.md",
        world: Some(setup_failed),
        steps: &[Step::Select(FOCUS), Step::OpenSetupLog],
    },
    Screen { name: "conversation", spec: "chats.md", world: Some(conversation), steps: &[Step::Select(FOCUS)] },
    Screen { name: "streaming", spec: "chats.md", world: Some(streaming), steps: &[Step::Select(FOCUS)] },
    Screen { name: "approval", spec: "chats.md", world: Some(approval), steps: &[Step::Select(FOCUS)] },
    Screen { name: "question", spec: "chats.md", world: Some(question), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "mode-picker",
        spec: "chats.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::Click("mode-picker")],
    },
    Screen { name: "diff", spec: "diff.md", world: Some(diff), steps: &[Step::Select(FOCUS), Step::OpenDiff] },
    Screen {
        name: "update-ready",
        spec: "updates.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::UpdateReady("1.0.1")],
    },
    Screen {
        name: "restart-to-update-question",
        spec: "updates.md",
        world: Some(conversation),
        steps: &[
            Step::Select(FOCUS),
            Step::Type("Also show it in the header"),
            Step::UpdateReady("1.0.1"),
            Step::Click("restart-to-update"),
        ],
    },
    Screen {
        name: "finishing-update",
        spec: "updates.md",
        world: Some(finishing_update),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "finish-update-question",
        spec: "updates.md",
        world: Some(finishing_update),
        steps: &[Step::Select(FOCUS), Step::AskToFinish],
    },
    Screen { name: "own-hub", spec: "updates.md", world: Some(own_hub), steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "update-problem",
        spec: "updates.md",
        world: Some(conversation),
        steps: &[Step::Select(FOCUS), Step::UpdateProblem],
    },
];

/// The conversation, on an older hub that a client started, with chats still running on it.
fn finishing_update() -> World {
    older_hub(true)
}

/// The conversation, on an older hub that someone runs themselves.
fn own_hub() -> World {
    older_hub(false)
}

fn older_hub(on_demand: bool) -> World {
    let mut world = conversation();
    world.hub = Some(HubBuild { version: "0.9.0", number: 1, on_demand });
    world
}

/// Two repositories, with a workspace in each state a sidebar row can show, and an archived one that stays hidden.
/// Returns the screen's own workspace, which comes after them.
fn sidebar(world: &mut World) -> WorkspaceId {
    let roundhouse = world.repo("roundhouse");
    let signal_box = world.repo("signal-box");
    for (name, status) in
        [("alder", SessionStatus::Idle), ("birch", SessionStatus::Running), ("cedar", SessionStatus::WaitingApproval)]
    {
        let id = world.workspace(&roundhouse, name);
        world.chat(&id, status, Vec::new());
    }
    let focus = world.workspace(&roundhouse, FOCUS);
    for (name, status) in [("elm", SessionStatus::Interrupted), ("fir", SessionStatus::Idle)] {
        let id = world.workspace(&signal_box, name);
        world.chat(&id, status, Vec::new());
    }
    let archived = world.workspace(&signal_box, "hazel");
    world.workspace_mut(&archived).status = WorkspaceStatus::Archived;
    focus
}

fn conversation() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.workspace_mut(&focus).branch = "sidebar-pr-state".to_string();
    world.chat(&focus, SessionStatus::Idle, chats::finished());
    world.pull_requests.insert(
        focus,
        PullRequest {
            number: 182,
            url: "https://example.com/pull/182".to_string(),
            title: "Show PR state in the sidebar".to_string(),
            state: "OPEN".to_string(),
            is_draft: false,
        },
    );
    world
}

fn streaming() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let chat = world.chat(&focus, SessionStatus::Running, chats::streaming());
    world.tails.insert(chat, chats::streaming_tail());
    world
}

fn approval() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.chat(&focus, SessionStatus::WaitingApproval, chats::approval());
    world
}

fn question() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.chat(&focus, SessionStatus::WaitingApproval, chats::question());
    world
}

fn diff() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.chat(&focus, SessionStatus::Idle, chats::finished());
    world.diffs.insert(
        focus.clone(),
        GitDiff { base_commit: Some("3f4c19c".to_string()), patch: chats::PATCH.to_string(), truncated: false },
    );
    world.comments.insert(
        focus.clone(),
        vec![DiffComment {
            id: CommentId::from("comment-1"),
            workspace_id: focus,
            path: "crates/protocol/src/model.rs".to_string(),
            side: DiffSide::New,
            line: 101,
            text: "Should this say when it was last checked, so a stale state can show as stale?".to_string(),
            created_at_ms: 0,
            rev: Rev::default(),
        }],
    );
    world
}

/// A new workspace whose setup script failed, with its log open and nothing said in its chat yet.
fn setup_failed() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.workspace_mut(&focus).setup = SetupStatus::Failed { code: Some(1) };
    world.chat(&focus, SessionStatus::Idle, Vec::new());
    world.setup_logs.insert(focus, chats::SETUP_LOG.to_string());
    world
}
