//! The database's migrations since its floor, whose schema `at_floor.sql` has. `before.rs` has how many there were
//! before some of them, for tests.

mod account_defaults;
mod accounts;
mod conversations;
mod drafts;
mod event_kinds;
mod github_accounts;
mod proposals;
mod session_accounts;
mod sessions;
mod todos;
mod waits;
mod workspaces;

mod sequence;
pub(super) use sequence::{AT_FLOOR, MIGRATIONS};
