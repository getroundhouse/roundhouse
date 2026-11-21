//! SQLite storage. Synchronous, and only called through `spawn_blocking`.
//!
//! All the database's SQL lives in this module, so it's the one thing a Postgres-backed hub would replace. The cache
//! of what the hub reads from GitHub is a file of its own, apart from the database, in `crate::github_cache`.

#[cfg(test)]
use protocol::model::{Seq, Session};
#[cfg(test)]
use std::path::Path;

mod accounts;
mod approvals;
mod attachments;
mod backup;
mod builds;
mod children;
mod claims;
pub(crate) mod columns;
mod comments;
mod drafts;
mod error;
mod events;
mod find;
mod github_accounts;
mod github_apps;
mod github_reach;
mod github_sign_ins;
mod hub_settings;
mod instances;
mod lines;
mod logins;
mod made;
#[cfg(any(test, feature = "testing"))]
mod migrated;
mod new_workspace_draft;
#[cfg(test)]
pub(crate) mod older;
mod prompts;
mod proposals;
mod queue;
mod reading;
mod records;
mod replies_heard;
mod repos;
mod run_logins;
#[cfg(test)]
mod same_again;
mod schema;
mod sent;
mod sessions;
mod subagents;
#[cfg(test)]
pub(crate) mod tests;
mod titles;
mod tools;
mod waits;
mod workspaces;

pub use accounts::Launched;
pub use attachments::AddedBy;
pub use comments::Commented;
pub use error::{BackupError, StoreError};
pub use events::{Initialized, Places, SessionChange, StatusChange, TurnEdge, cancelled_approval};
pub(crate) use github_accounts::{GithubConnected, GithubConnecting, GithubRemoved};
pub(crate) use github_apps::GithubAppMaking;
pub(crate) use github_sign_ins::{GithubApproved, GithubApproving, StartedSignIn};
pub use instances::{InstanceStage, RecordedInstance};
pub(crate) use logins::{Connected, Connecting, Held, IdFrom};
pub use made::{Made, MadeByRequest};
pub use proposals::{AnswerToKeep, Answering, NewRevision, Revising, Withdrawing};
pub use queue::{Changed, Queued, Queuing, Unqueued};
pub use reading::Reading;
pub use repos::RepoToRead;
pub(crate) use run_logins::Placed;
pub(crate) use sessions::SessionLogin;
pub use titles::{Retitle, Retitled};
pub use workspaces::{BranchesSet, Ended, Move, WorkspacePlacement};

mod database;
use columns::{At, ByColumn, Column, Columns, OneValue, columns};
pub(crate) use database::later_ms;
pub use database::{Appended, Store, StoredSession, now_ms};
use database::{
    current_rev, json_at, json_from, lock, maybe_text_at, place_column, place_of, required_json_at, rev_of, seq_column,
    seq_of, text_at, text_from, text_of,
};
#[cfg(any(test, feature = "testing"))]
pub use migrated::give_a_migrated_database;

mod devices;
pub(crate) use devices::TokenHash;
