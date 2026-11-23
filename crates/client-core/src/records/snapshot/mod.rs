//! What a view reads of one hub's records: a copy as of one moment, which doesn't change, and is shared, so holding one
//! costs a pointer. Each kind of record is listed in the order views show it, and found by its ID, and a view keeps the
//! snapshot it last looked at, to tell what's changed since, as [`Snapshot::changes_since`] says.
//!
//! It's made the first time it's asked for after a change, so changes that come between two looks are made into one,
//! and made again only for the side that changed: the records, or the subagents, the proposals or the replies heard of
//! the sessions views show, or the observations.

#[cfg(test)]
mod tests;

mod by_session;
mod copies;
mod github;
mod observed;
mod of_accounts;
mod of_chats;
mod of_workspaces;
pub(super) use by_session::SessionsListed;
pub(super) use copies::Listed;
pub use copies::{Changes, InWorkspace, Snapshot};
pub(super) use observed::ObservationsListed;
pub use of_accounts::AccountIdentity;
