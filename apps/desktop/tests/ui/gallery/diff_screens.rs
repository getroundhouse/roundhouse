//! The screens of the diff and the workspace's files, apart from the rest in `screens.rs` only to keep each list a
//! readable length.

#![cfg(test)]

use super::frame::{CHANGES, FILE, Frame};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worktree;
use super::worlds::{conversation, diff};

pub(crate) const DIFF_SCREENS: &[Screen] = &[
    Screen { name: "changes", spec: "diff.md", world: Some(diff), frame: CHANGES, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "changes-hidden",
        spec: "diff.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::Click("toggle-diff")],
    },
    Screen {
        name: "changes-tree",
        spec: "diff.md",
        world: Some(diff),
        frame: CHANGES,
        steps: &[Step::Select(FOCUS), Step::Click("changes-layout")],
    },
    Screen {
        name: "diff",
        spec: "diff.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs")],
    },
    Screen {
        name: "writing-a-comment",
        spec: "diff.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs"), Step::StartComment(99)],
    },
    Screen {
        name: "all-files",
        spec: "files.md",
        world: Some(diff),
        frame: CHANGES,
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-crates"),
            Step::Click("all-folder-crates/protocol"),
            Step::Click("all-folder-crates/protocol/src"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "file-opened",
        spec: "files.md",
        world: Some(diff),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-crates"),
            Step::Click("all-folder-crates/protocol"),
            Step::Click("all-folder-crates/protocol/src"),
            Step::ViewFile(worktree::OPENED),
            Step::MoveAway,
        ],
    },
];
