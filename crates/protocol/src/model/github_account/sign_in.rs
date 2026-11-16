use serde::{Deserialize, Serialize};

use super::super::{GithubAccountId, GithubSignInId, GithubUserCode, Rev};

/// A sign-in with GitHub that the hub runs, through GitHub's device flow, as a record of its own, so every client shows
/// the same of it, and a client that reconnects or starts again finds it as it is. The hub asks GitHub until the
/// person approves or refuses, or the code ends, whether or not any client is there. A sign-in approved as a GitHub
/// user the hub holds a sign-in for renews that account's login in place, and any other makes a new account.
///
/// Once it has ended, its record stays, so a client showing it can say how it ended, until another sign-in starts on
/// the hub, and then goes, as `HubNotice::Removed` says. A waiting sign-in doesn't outlive its hub, since only that hub
/// held what GitHub is asked with, so the next hub records it as ended as it starts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubSignIn {
    pub id: GithubSignInId,
    pub status: GithubSignInStatus,
    #[serde(default)]
    pub rev: Rev,
}

/// Where a sign-in with GitHub has got to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GithubSignInStatus {
    /// Waiting for the person to type `code` at `address`, such as `https://github.com/login/device`, and approve,
    /// until the code ends at `code_ends_at_ms`, by the hub's clock.
    Waiting { code: GithubUserCode, address: String, code_ends_at_ms: i64 },
    /// Approved, as the GitHub account `account`, which it made, or whose login it renewed.
    Approved { account: GithubAccountId },
    /// The person refused it on GitHub.
    Refused,
    /// Its code ended before the person approved it, or the hub stopped while it waited.
    Ended,
    /// A client cancelled it.
    Cancelled,
    /// GitHub answered with something the hub can't go on from, or the hub couldn't keep what GitHub gave, as
    /// `message` says.
    Failed { message: String },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl GithubSignInStatus {
    /// Whether it's still waiting for the person, so cancelling it would change it.
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting { .. })
    }
}
