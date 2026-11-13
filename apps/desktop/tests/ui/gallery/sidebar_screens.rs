//! More of the sidebar's screens, each of a state its rules in `sidebar.md` say, apart from the rest in `screens.rs`
//! only to keep each list a readable length.

#![cfg(test)]

use super::frame::{SIDEBAR, SIDEBAR_MENU};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::conversation;

/// What's typed in the screen's own workspace and left unsent.
const DRAFT: &str = "Keep the badge at its own width";

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "sidebar-on-home",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Type(DRAFT), Step::Click("home"), Step::MoveAway],
    },
    Screen {
        name: "sort-menu",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: SIDEBAR_MENU,
        steps: &[Step::Select(FOCUS), Step::Point("workspaces-heading"), Step::Click("sort-workspaces")],
    },
    Screen {
        name: "sidebar-edge",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Point("left-sidebar-edge")],
    },
    Screen {
        name: "row-end",
        spec: "sidebar.md",
        world: Some(expiring),
        frame: SIDEBAR,
        // Typed in the screen's own workspace, the fourth, which is then left for another, so its row has a pencil.
        steps: &[Step::Select(FOCUS), Step::Type(DRAFT), Step::Select("alder"), Step::Point("workspace-ws-4")],
    },
];

/// The conversation, whose chat's cache is about to expire, nine minutes from now.
fn expiring() -> World {
    let mut world = conversation();
    let chat = world.sessions.iter().rfind(|session| session.last_seq > 0).unwrap().id.clone();
    world.cached(&chat, 51);
    world
}
