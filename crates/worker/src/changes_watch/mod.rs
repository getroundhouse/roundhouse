//! Watching a worktree's files, so its hub has the worktree's change summary within a moment of a file changing,
//! whoever changed it: an agent mid-turn, a person in their editor, or git in a terminal. The OS says when a file git
//! doesn't ignore changes, so a build writing into `target/` wakes nothing, and the summary is worked out once the
//! writes stop, and sent only if it differs from the last one sent. It's read every few minutes too, in case the OS
//! missed something, and after every folder that appears, since the OS can't say what was written in a folder before
//! it was watched. The hub asks for it, as `protocol::worker::WatchChanges` says, while a client follows the workspace.

#[cfg(any(target_os = "linux", test))]
mod each_folder;
#[cfg(any(not(target_os = "linux"), test))]
mod from_the_top;
mod heard;
mod ignored;
mod system;
#[cfg(test)]
mod tests;
mod watching;

pub(crate) use watching::{Watched, watch};
