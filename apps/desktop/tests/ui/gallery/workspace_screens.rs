//! More of the screens about making a workspace and titling it, each of a state its rules in `workspaces.md` say,
//! apart from the rest in `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use look::Region;
use protocol::model::{SetupStatus, Title, WorkspaceStatus};

use super::frame::{Frame, Part, SIDEBAR, SIDEBAR_MENU};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::{conversation, sidebar, titled};

/// What's typed into the dialog before Create, for a workspace the screen's hub never finishes making.
const TASK: &str = "Keep the PR badge at its own width when the sidebar is narrow";

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "new-workspace-projects",
        spec: "workspaces.md",
        world: Some(titled),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::Click("new-in-repo-1"), Step::Click("new-workspace-repo")],
    },
    Screen {
        name: "new-workspace-menu",
        spec: "workspaces.md",
        world: Some(titled),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::Click("new-in-repo-1"), Step::Click("new-workspace-add")],
    },
    Screen {
        name: "creating-workspace",
        spec: "workspaces.md",
        world: Some(creating),
        frame: SIDEBAR,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Type(TASK),
            Step::Click("new-workspace-create"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "new-workspace-waiting",
        spec: "workspaces.md",
        world: Some(creating),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Type(TASK),
            Step::Click("new-workspace-create"),
            // The project's own `+` waits while one is being made in it, and the sidebar's Create opens the dialog.
            Step::Click("create"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "setting-up",
        spec: "workspaces.md",
        world: Some(setting_up),
        frame: Frame::Around(Part::Region(Region::SetupLog), &[Part::Region(Region::Composer)]),
        steps: &[Step::Select(FOCUS), Step::OpenSetupLog],
    },
    Screen {
        name: "titles-in-the-sidebar",
        spec: "workspaces.md",
        world: Some(some_titled),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "renaming-in-the-sidebar",
        spec: "workspaces.md",
        world: Some(titled),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Click("regenerate-title"), Step::MoveAway],
    },
    Screen {
        name: "long-title",
        spec: "workspaces.md",
        world: Some(long_titled),
        frame: Frame::Exactly(Part::Region(Region::Header), &[]),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "untitled-workspace-menu",
        spec: "workspaces.md",
        world: Some(conversation),
        // The screen's own workspace, the fourth, which nothing has titled yet.
        frame: SIDEBAR_MENU,
        steps: &[Step::Select(FOCUS), Step::RightClick("workspace-ws-4")],
    },
];

/// Every workspace titled, and one more in `roundhouse` that the hub is making, as its record says, whichever app
/// asked for it.
fn creating() -> World {
    let mut world = titled();
    let roundhouse = world.repos[0].id.clone();
    let made = world.workspace(&roundhouse, "gale");
    let workspace = world.workspace_mut(&made);
    workspace.status = WorkspaceStatus::Creating;
    // Its worktree isn't made yet.
    workspace.path = None;
    world
}

/// A workspace whose setup is still running, partway through its log.
fn setting_up() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    world.workspace_mut(&focus).setup = SetupStatus::Running;
    world.chat(&focus, protocol::model::SessionStatus::Idle, Vec::new());
    let log = "Copying .env\nCopying .env.local\n$ npm install\nadded 1,204 packages in 38s\n$ npm run build\n";
    world.setup_logs.insert(focus, log.to_string());
    world
}

/// Workspaces titled each way a title comes: generated, from the first line of a long prompt while the generated one
/// is on its way, and none yet, so the row shows the workspace's name.
fn some_titled() -> World {
    let mut world = conversation();
    let long_prompt = "When the sidebar is narrow, the PR badge runs over the workspace's name. Let the name shrink.";
    let titles = [
        ("alder", Title::tidied("Fix the login redirect")),
        ("birch", Title::from_first_line(long_prompt)),
        (FOCUS, Title::tidied("Show PR state in the sidebar")),
        ("elm", Title::tidied("Retry a failed setup")),
    ];
    for workspace in &mut world.workspaces {
        workspace.title = titles.iter().find(|(name, _)| *name == workspace.name).and_then(|(_, title)| title.clone());
    }
    world
}

/// Every workspace titled, the screen's own with a title too long for its header.
fn long_titled() -> World {
    let mut world = titled();
    let long =
        "Let the workspace's name shrink and end in an ellipsis when the sidebar is narrow, and keep the badge whole";
    for workspace in world.workspaces.iter_mut().filter(|workspace| workspace.name == FOCUS) {
        workspace.title = Title::tidied(long);
    }
    world
}
