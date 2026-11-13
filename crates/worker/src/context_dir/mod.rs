//! A worktree's `.context/` folder, where what isn't the repository's goes, as opposed to an agent's context, which is
//! what its model holds. The worker gives it, and each folder of it that the worker writes to, a `.gitignore` that
//! ignores everything in it, itself included, so nothing there shows as a change and nothing about the repository has
//! to change. A `.gitignore` the repository keeps in `.context/` is kept instead, and decides what's ignored there. So
//! `.context/attachments/` has one of its own, which also covers a prompt's files put there before any agent has
//! started in the worktree, and so before `.context/` has its `.gitignore`. Files the repository tracks there stay
//! tracked, though a new one needs `git add -f`. They're deleted with the worktree when it's archived. Nothing else in
//! `.context/` is touched, since agents and the repository keep things of their own there.

#[cfg(test)]
use std::io;

#[cfg(test)]
mod tests;

mod folder;
pub(crate) use folder::Folder;
