//! What a client shows of GitHub, which it tells the hub, and what the hub sends it as that changes, so no client asks
//! again on a timer.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, Request};
use crate::model::{PullRequestFound, ReferenceAsk, ReferenceRead, WorkspaceId};

/// Follows what GitHub says of what [`GithubShown`] names. The hub sends a [`GithubUpdate`] for each of them it knows
/// something of, then another each time what it knows changes, and reads them from GitHub more often than what no
/// client shows. Sent again under the same `subscription_id`, it replaces what's shown, and the hub sends what it
/// knows of each that's newly shown. What's shown ends with the subscription, or its connection, so after a reconnect a
/// client subscribes again. Answered with `Ok`. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubSubscribe {
    pub subscription_id: u64,
    pub shown: GithubShown,
}

impl Request for GithubSubscribe {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubSubscribe(self)
    }
}

/// The most issues and pull requests one subscription can show as chips, which is more than a window shows, so a hub
/// refuses a subscription that shows more.
pub const MOST_REFERENCES_SHOWN: usize = 200;

/// What a client shows of GitHub, all of it, each time it says.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct GithubShown {
    /// The workspaces whose own pull requests it shows, as their marks in a sidebar do.
    #[serde(default)]
    pub workspaces: Vec<WorkspaceId>,
    /// Those of `workspaces` whose pull request it shows in full, as the pull request's bar does, which the hub reads
    /// more often while it's open.
    #[serde(default)]
    pub closely: Vec<WorkspaceId>,
    /// The issues and pull requests it shows as chips, by their numbers.
    #[serde(default)]
    pub references: Vec<ReferenceAsk>,
}

/// What the hub knows now of one of what a [`GithubSubscribe`] shows, sent as it changes.
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
    },
    /// From a newer hub, which this client skips.
    #[serde(other)]
    Unknown,
}
