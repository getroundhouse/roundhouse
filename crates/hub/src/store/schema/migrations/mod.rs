//! The database's migrations, from the first, which `first.rs` has. `before.rs` has how many there were before some of
//! them, for tests.

mod account_defaults;
mod accounts;
mod activity;
mod agent_runs;
mod cache;
mod comments;
mod devices;
mod drafts;
mod first;
mod github_accounts;
mod github_sign_ins;
mod hub_settings;
mod limits;
mod logins;
mod providers;
mod replies;
mod repos;
mod session_accounts;
mod subagents;
mod tools;
mod typed_calls;
mod typed_requests;
mod waiting;
mod waits;
mod workers;
mod workspaces;

mod sequence;
pub(super) use sequence::MIGRATIONS;
