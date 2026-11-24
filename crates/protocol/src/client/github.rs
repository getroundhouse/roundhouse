//! What a client shows of GitHub, which it tells the hub, and what the hub sends it as that changes, so no client asks
//! again on a timer.

use serde::{Deserialize, Serialize};

use crate::model::{PullRequestFound, ReferenceAsk, ReferenceRead, RepositoryUnseen, WorkspaceId};

/// The most issues and pull requests one subscription can show as chips, which is more than a window shows, so a hub
/// refuses a subscription that shows more.
pub const MOST_REFERENCES_SHOWN: usize = 200;

/// What the hub knows now of an issue or a pull request a chip shows, or of a workspace's own pull request, sent to a
/// client of version 4 as it changes, in place of the observations a client of version 5 reads.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "of", rename_all = "snake_case")]
#[expect(clippy::large_enum_variant, reason = "an update is sent as it's made, and taken apart as it's read")]
pub enum GithubUpdate {
    /// What GitHub says of an issue or a pull request a chip shows. Sent as `referenced`, since what it says changed
    /// form, and a client from before skips a kind it doesn't know.
    #[serde(rename = "referenced")]
    Reference { reference: ReferenceAsk, read: ReferenceRead },
    /// A workspace's own pull request, with how its worktree stands against it, or that its branch has none. Sent as
    /// `own_pull_request`, as `Reference` is sent apart.
    #[serde(rename = "own_pull_request")]
    PullRequest {
        workspace: WorkspaceId,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::model::deserialize_looked"
        )]
        found: Option<PullRequestFound>,
        /// That GitHub showed the default no repository where the branch's pull requests are, as the hub last read
        /// it, so `found` is a pull request found before, if one was, and otherwise says the hub hasn't looked, since a
        /// none found before is no longer known. A client from before skips it, and reads `found`. `None` from a hub
        /// from before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unseen: Option<RepositoryUnseen>,
    },
    /// From a newer hub, which this client skips.
    #[serde(other)]
    Unknown,
}
