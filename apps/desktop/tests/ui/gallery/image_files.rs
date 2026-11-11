//! The branch the screens of changed images review: it fixes a badge in the sidebar, so the spec's screen of it changes,
//! and it adds another screen.

#![cfg(test)]

use protocol::model::{CommitId, GitDiff};

use super::worktree::{SHOT_HEIGHT, png};
use super::world::World;
use super::worlds::{finished_chat, sidebar};
use super::{patch, pictures};

/// The screen the branch changes, which the screens open.
pub(crate) const CHANGED: &str = "docs/spec/screens/sidebar.png";
/// The screen it adds.
const ADDED: &str = "docs/spec/screens/sidebar-narrow.png";

pub(super) fn changed_images() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    finished_chat(&mut world, &focus);
    let base_commit = Some(CommitId::from(patch::BASE_COMMIT));
    world.diffs.insert(focus, GitDiff { base_commit, patch: patch::IMAGES.to_string(), truncated: false });
    world.base_files.insert(CHANGED.to_string(), png(pictures::badge(SHOT_HEIGHT, true)));
    world.worktree_files.insert(CHANGED.to_string(), png(pictures::badge(SHOT_HEIGHT, false)));
    world.worktree_files.insert(ADDED.to_string(), png(pictures::badge(SHOT_HEIGHT, false)));
    world
}
