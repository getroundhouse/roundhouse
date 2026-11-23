//! The file the hub keeps its copies of what others own in, apart from its database: what it last read from GitHub,
//! and its observations. The hub empties it rather than migrates it, as `docs/principles/observations.md` says, and
//! each part of it says the form of its own tables, so a build that changes one part's form, or a part whose tables
//! can't be made, empties only that part.

mod failure;
mod file;
mod parts;
#[cfg(test)]
mod tests;

pub(crate) use file::CacheFile;
pub(crate) use parts::{Part, drop_named, prepared_or_emptied};
