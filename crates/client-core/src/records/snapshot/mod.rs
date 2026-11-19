//! What a view reads of one hub's records: a copy as of one moment, which doesn't change, and is shared, so holding one
//! costs a pointer. Each kind of record is listed in the order views show it, and found by its ID, and a view keeps the
//! snapshot it last looked at, to tell what's changed since, as [`Snapshot::changes_since`] says.
//!
//! It's made the first time it's asked for after a change, so changes that come between two looks are made into one,
//! and made again only for the side that changed: the records, or the subagents or the proposals of the sessions views
//! show.

#[cfg(test)]
mod tests;

mod by_session;
mod copies;
mod github;
pub(super) use by_session::SessionsListed;
pub(super) use copies::Listed;
pub use copies::{Changes, InWorkspace, Snapshot};
