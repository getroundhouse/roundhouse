//! More of Settings' screens, each of a state its rules in `settings.md` say, apart from the rest in `screens.rs` only
//! to keep each list a readable length.

#![cfg(test)]

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::{conversation, own_hub};

/// What puts the cursor in Settings' search.
const FIND: &str = "secondary-f";

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "settings-search",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, Step::Press(FIND), Step::Type("sound")],
    },
    Screen {
        name: "settings-nothing-found",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, Step::Press(FIND), Step::Type("zebra")],
    },
    Screen {
        name: "settings-older-hub",
        spec: "settings.md",
        world: Some(settings_unkept),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings],
    },
];

/// An older hub, from before hubs kept settings.
fn settings_unkept() -> World {
    let mut world = own_hub();
    world.keeps_no_settings = true;
    world
}
