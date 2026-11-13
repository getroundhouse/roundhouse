//! Which pull request is a branch's. `gh` finds one by the branch's name, and a name is only one repository's:
//! another machine's hub names its workspaces from the same list, so a branch here can share its name with one pushed
//! from there and opened as a pull request. Taking that one for this branch's would show it on a workspace it has
//! nothing to do with, and offer to push this worktree's work into it.
//!
//! So a pull request is a branch's once the branch has been at one of its commits, which a commit made on the branch
//! or a push from it leaves in the branch's history, and so does checking out someone else's branch to work on it.
//! One that only shares the name has commits the branch was never at, even once they're fetched or merged in, or
//! only started from, when this one was made from a remote's branch other than the pull request's, as a workspace is
//! made from `origin`'s default branch, which [`has_been_at`] doesn't count. From
//! then on it's the branch's by a link, since a pull request can move on without any of the branch's commits, as when
//! someone else rewrites it. The link is one of the branch's settings in git's config, the pull request's URL, so git
//! keeps it with the branch: renamed with it, there for every worktree and after the worktree is gone, and deleted
//! with it, so a branch made again by an old name starts with none. A branch copied with `git branch -c`, or reset
//! with `git switch -C`, keeps the link it had, and one deleted from under git's own commands, as by
//! `git update-ref -d`, leaves it behind for a branch made again by its name.

#[cfg(test)]
mod tests;

mod branch_link;

pub(super) use branch_link::{Choice, choose, has_been_at, link, linked};
#[cfg(test)]
pub(super) use branch_link::{made_from, remote_branch};
