//! What a client holds of one hub's records: its repositories, workspaces, sessions and comments, its settings, the
//! accounts its agents run under, and the subagents and proposals of the sessions a view is showing. And the hub's
//! observations, kept by the same rule as its records. Beside them, what
//! each kind of agent the hub runs is called and offers, which isn't a record, since it doesn't change while the hub
//! runs. Views read them all through a [`Snapshot`].
//!
//! No IO. Lists, feed notices and replies can arrive in any order, and each record keeps whichever copy has the newer
//! revision, so the result is the same however they interleave.

#[cfg(test)]
use protocol::client::HubNotice;
#[cfg(test)]
use protocol::model::{RecordKey, Records, Rev};

mod blanking;
mod by_revision;
#[cfg(test)]
mod laws;
mod of_session;
mod snapshot;

pub use of_session::subagent_named;
pub use snapshot::{AccountIdentity, Changes, GithubName, InWorkspace, ListOfModels, Snapshot, WorkerBuild};

mod cache;
pub use cache::HubRecords;
use cache::{Kept, Taken};
