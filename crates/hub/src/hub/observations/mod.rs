//! The hub's observations, its copies of what others own, as it last observed them: one store for every kind, in
//! memory and in the cache file, [`crate::cache_file`], with a table and a version for each kind, so a build that
//! changes one kind's form empties only that kind. `docs/principles/observations.md` has the rules. The code that reads
//! each kind fills it, as `super::workspace_lines` does a workspace's lines added and removed.

mod changes;
mod file;
mod kept;
mod kinds;
mod levels;
#[cfg(test)]
mod tests;

pub(crate) use file::prepare;
pub(crate) use kept::Observations;
pub(crate) use kinds::{WorkspaceChanges, WorkspaceLines};
