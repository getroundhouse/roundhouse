//! The screens of the diff and the workspace's files, apart from the rest in `screens.rs` only to keep each list a
//! readable length.

#![cfg(test)]

use look::Region;

use super::frame::{CHANGES, FILE, Frame, Part, TABS};
use super::image_files::{CHANGED, changed_images};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{conversation, diff, own_hub, videos};
use super::{patch, worktree};

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
        name: "diff-split",
        spec: "diff.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs"), Step::Click("diff-split")],
    },
    Screen {
        name: "diff-whole-file",
        spec: "diff.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs"), Step::Click("diff-whole-file")],
    },
    Screen {
        name: "diff-rendered",
        spec: "diff.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile(patch::SPEC), Step::Click("diff-rendered"), Step::MoveAway],
    },
    Screen {
        name: "file-tab",
        spec: "diff.md",
        world: Some(diff),
        frame: TABS,
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs"), Step::MoveAway],
    },
    Screen {
        name: "file-path-copy",
        spec: "diff.md",
        world: Some(diff),
        frame: Frame::Around(Part::Region(Region::TabBar), &[Part::Region(Region::FileBar)]),
        steps: &[
            Step::Select(FOCUS),
            Step::OpenFile("crates/protocol/src/model.rs"),
            Step::Point(Region::FileBar.id()),
        ],
    },
    Screen {
        name: "writing-a-comment",
        spec: "diff.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile("crates/protocol/src/model.rs"), Step::StartComment(35)],
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
    Screen {
        name: "image-opened",
        spec: "files.md",
        world: Some(diff),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-.context/gallery"),
            Step::ViewFile(worktree::PICTURE),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "video-opened",
        spec: "files.md",
        world: Some(videos),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-.context/gallery"),
            Step::ViewFile(worktree::RECORDING),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "video-for-the-system",
        spec: "files.md",
        world: Some(videos),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::Click("all-folder-.context"),
            Step::WaitFor("all-folder-.context/gallery"),
            Step::Click("all-folder-.context/gallery"),
            Step::ViewFile(worktree::RECORDING_MP4),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "markdown-opened",
        spec: "files.md",
        world: Some(diff),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::AllFiles, Step::ViewFile(worktree::README), Step::MoveAway],
    },
    Screen {
        name: "markdown-source",
        spec: "files.md",
        world: Some(diff),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::AllFiles,
            Step::ViewFile(worktree::README),
            Step::Click("markdown-source"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "all-files-older-hub",
        spec: "files.md",
        world: Some(own_hub),
        frame: CHANGES,
        // A hub too old to list files has the sidebar start on Changes, which stands in for All files.
        steps: &[Step::Select(FOCUS), Step::Click("sidebar-tab-all-files"), Step::MoveAway],
    },
    Screen {
        name: "image-side-by-side",
        spec: "diff.md",
        world: Some(changed_images),
        frame: FILE,
        steps: &[Step::Select(FOCUS), Step::OpenFile(CHANGED), Step::WaitFor("images-side-by-side"), Step::MoveAway],
    },
    Screen {
        name: "image-flip",
        spec: "diff.md",
        world: Some(changed_images),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenFile(CHANGED),
            Step::Click("image-after"),
            Step::WaitFor("images-flip"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "image-difference",
        spec: "diff.md",
        world: Some(changed_images),
        frame: FILE,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenFile(CHANGED),
            Step::Click("image-difference"),
            Step::WaitFor("images-difference"),
            Step::MoveAway,
        ],
    },
];
