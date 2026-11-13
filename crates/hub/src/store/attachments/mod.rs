//! What the files a session's prompts carry are, and the images its agent's tools send, and where in the worktree each
//! of a prompt's files was put, and the calls that take a file whatever it's for, a session or the new-workspace
//! dialog. What a file is never changes once it's added. Its bytes aren't here, but in a file of their own, which
//! `attachment_files` keeps.

#[cfg(test)]
use super::{StoreError, lock};
#[cfg(test)]
use protocol::model::SessionId;
#[cfg(test)]
use rusqlite::params;

#[cfg(test)]
mod tests;

mod attachment_rows;
pub use attachment_rows::AddedBy;
pub(super) use attachment_rows::named_on;
