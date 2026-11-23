//! What a worktree changes against its base, as [`protocol::model::ChangeSummary`] says: git's numbers for each file it
//! tracks, with no patch, beside the untracked files, which the worker counts itself, and a version of each file the
//! worktree holds, which it reads again only once the file's size or modified time differs.

mod listing;
mod reading;
mod seen;
mod summaries;
#[cfg(test)]
mod tests;

pub(super) use listing::{tracked, untracked};
pub(super) use reading::version_now;
pub(crate) use summaries::ChangeSummaries;
