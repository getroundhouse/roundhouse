//! What the hub last read from GitHub, kept for every part of the hub that reads it, so the same issue or pull request
//! is read once however many ask, by what it describes and in facets that each have the time they were read, as
//! `docs/principles/outside-systems.md` says. It's in memory, and in the cache file, [`crate::cache_file`], so a hub
//! that starts again has it, under a version of its own: everything in it can be read from GitHub again.

mod file;
mod kept;
mod keys;
mod pulls;
mod rules;
#[cfg(test)]
mod tests;
mod told;

pub(crate) use file::prepare;
pub(crate) use kept::{GithubCache, KeptSummary};
pub(crate) use keys::{Facet, Item};
pub(crate) use told::Told;
