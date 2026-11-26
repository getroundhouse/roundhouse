//! Following workspaces at their own level, while a handle on each is kept: what's there is what only a workspace's own
//! pane draws, such as its changed files, and its pull request's checks, history and stack, and following it is what
//! has the hub's worker watch its files and the hub read GitHub of it more often. What's sent goes into the copy of the
//! hub's records, which views read.

mod followed;
#[cfg(test)]
mod tests;

pub use followed::FollowedWorkspace;
pub(super) use followed::Workspaces;
