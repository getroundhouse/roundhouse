//! SQLite storage. Synchronous, and only called through `spawn_blocking`.
//!
//! All SQL lives in this module, so it's the one thing a Postgres-backed hub would replace.

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
mod comments;
mod drafts;
mod error;
mod events;
mod find;
mod hub_settings;
mod instances;
mod lines;
mod logins;
mod made;
mod new_workspace_draft;
#[cfg(test)]
mod older;
mod previous;
mod prompts;
mod queue;
mod reading;
mod records;
mod repos;
mod run_logins;
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
pub use instances::{InstanceStage, RecordedInstance};
pub(crate) use logins::{Connecting, Held, IdFrom};
pub use made::{Made, MadeByRequest};
pub use queue::{Changed, Queued, Queuing};
pub use reading::Reading;
pub use titles::{Retitle, Retitled};
pub use waits::Seen;
pub use workspaces::{BranchesSet, Ended, Move};

mod database;
pub use database::{Appended, Store, StoredSession, now_ms};
use database::{current_rev, from_text, json_at, lock, place_column, place_of, rev_of, seq_column, seq_of, text_of};
