//! Screens of the rules every screen follows, in `look.md`, where no area's own screen shows one by itself.

#![cfg(test)]

use look::Region;

use super::frame::{Frame, Part};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;
use super::worlds::titled;

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "button-hovered",
        spec: "look.md",
        world: Some(titled),
        frame: Frame::Exactly(Part::Region(Region::Header), &[]),
        steps: &[Step::Select(FOCUS), Step::Point("toggle-diff")],
    },
    // About the pane, which no region marks on its own, so it's the whole window, as the window before there's a
    // project is.
    Screen { name: "start-a-workspace", spec: "look.md", world: Some(projects_only), frame: Frame::Layout, steps: &[] },
];

/// Two projects, and no workspace in either.
fn projects_only() -> World {
    let mut world = World::default();
    world.repo("roundhouse");
    world.repo("signal-box");
    world
}
