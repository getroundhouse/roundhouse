//! The hub's observations, its copies of what others own, as it last observed them: one store for every kind, in
//! memory and in the cache file, [`crate::cache_file`], with a table and a version for each kind, so a build that
//! changes one kind's form empties only that kind. `docs/principles/observations.md` has the rules. Nothing fills it
//! yet: `docs/principles/serving.md` names the first kind, a workspace's lines added and removed, as not read yet.

mod changes;
mod file;
mod kept;
mod kinds;
#[cfg(test)]
mod tests;

pub(crate) use file::prepare;
pub(crate) use kept::Observations;
