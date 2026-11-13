//! More of Home's screens, each of a state its rules in `home.md` say, apart from the rest in `screens.rs` only to keep
//! each list a readable length.

#![cfg(test)]

use look::Region;
use protocol::model::{WorkspaceId, WorkspaceStatus};

use super::frame::{Frame, Part};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::{titled, unread_titled};

/// Home, and nothing more.
const HOME: Frame = Frame::Exactly(Part::Region(Region::Home), &[]);

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "home-unread",
        spec: "home.md",
        world: Some(unread_titled),
        frame: HOME,
        steps: &[Step::Select(FOCUS), Step::Click("home")],
    },
    Screen {
        name: "home-archived",
        spec: "home.md",
        world: Some(titled),
        frame: HOME,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("home"),
            Step::Click("home-archived"),
            Step::ClickItem("Showing all"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "home-search",
        spec: "home.md",
        world: Some(titled),
        frame: HOME,
        // The search has the keys while Home shows.
        steps: &[Step::Select(FOCUS), Step::Click("home"), Step::Type("fix")],
    },
    Screen {
        name: "home-archiving",
        spec: "home.md",
        world: Some(archiving),
        frame: HOME,
        steps: &[Step::Select(FOCUS), Step::Click("home")],
    },
];

/// Every workspace titled, and the first being archived, as its record says, whichever app asked.
fn archiving() -> World {
    let mut world = titled();
    world.workspace_mut(&WorkspaceId::from("ws-1")).status = WorkspaceStatus::Archiving;
    world
}
