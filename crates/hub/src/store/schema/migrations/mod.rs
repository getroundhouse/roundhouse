//! The database's migrations, from the first, which `first.rs` has. `before.rs` has how many there were before some of
//! them, for tests.

mod accounts;
mod activity;
mod cache;
mod comments;
mod drafts;
mod first;
mod hub_settings;
mod limits;
mod logins;
mod providers;
mod replies;
mod subagents;
mod tools;
mod waiting;
mod waits;
mod workers;
mod workspaces;

mod sequence;
pub(super) use sequence::MIGRATIONS;
