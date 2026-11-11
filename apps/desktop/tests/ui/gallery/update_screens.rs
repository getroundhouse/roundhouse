//! The screens of what someone sees when the app or its hub updates, or when there's no hub, apart from the rest in
//! `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use look::Region;

use super::frame::{Frame, Part, SIDEBAR};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::{conversation, finishing_update, own_hub};

pub(crate) const UPDATE_SCREENS: &[Screen] = &[
    Screen { name: "no-hub", spec: "updates.md", world: None, frame: Frame::Layout, steps: &[] },
    Screen {
        name: "update-ready",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::UpdateReady("1.0.1")],
    },
    Screen {
        name: "restart-to-update-question",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
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
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "update-details",
        spec: "updates.md",
        world: Some(finishing_update),
        frame: Frame::Exactly(Part::Region(Region::LeftSidebar), &[Part::Region(Region::UpdateDetails)]),
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("update-details"), shows: Region::UpdateDetails.id() },
        ],
    },
    Screen {
        name: "finish-update-question",
        spec: "updates.md",
        world: Some(finishing_update),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::AskToFinish],
    },
    Screen { name: "own-hub", spec: "updates.md", world: Some(own_hub), frame: SIDEBAR, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "update-problem",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::UpdateProblem],
    },
];
