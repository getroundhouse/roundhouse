//! What a client holds of one hub's records: its repositories, workspaces, sessions and comments, its settings, the
//! accounts its agents run under, and the subagents of the sessions a view is showing. Beside them, what each kind of
//! agent the hub runs is called and offers, which isn't a record, since it doesn't change while the hub runs. Views
//! read them all through a [`Snapshot`].
//!
//! No IO. Lists, feed notices and replies can arrive in any order, and each record keeps whichever copy
//! has the newer revision, so the result is the same however they interleave. A hub that doesn't keep
//! revisions sends 0, and then the copy that arrives last wins. One ordered connection keeps that
//! right for lists and notices, though a reply can still overtake a notice.

#[cfg(test)]
use protocol::client::HubNotice;
#[cfg(test)]
use protocol::model::{RecordKey, Records, Rev};

#[cfg(test)]
mod laws;
mod snapshot;
mod subagents;

pub use snapshot::{Changes, InWorkspace, Snapshot};

mod cache;
pub use cache::HubRecords;
use cache::{Kept, Taken};
