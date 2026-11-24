//! How a hub installed as a service's reads of the update feed, and an update a client asked for, are going.

use serde::{Deserialize, Serialize};

use super::Rev;

/// How a hub installed as a service's reads of the update feed are going, which it reads as it starts and every 10
/// minutes after, as `docs/spec/updates.md` says, and how an update a client asked for is going. The release the feed
/// offers is the observation `ReleaseOffered`. A hub has at most one. One that doesn't read the feed, such as one a
/// client started, has none. The hub keeps it in memory, and reads the feed again as it starts. *Unconfirmed:* the
/// shape.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubUpdate {
    /// When the hub last read the feed, or tried to, by the hub's clock, in milliseconds since the Unix epoch.
    pub looked_at_ms: i64,
    /// Why the last read failed, such as a feed that couldn't be reached, or whose signature didn't match, which leaves
    /// the release offered as the read before found it. `None` once a read worked.
    #[serde(default)]
    pub problem: Option<String>,
    /// How the update a client last asked for is going, which the hub's reads of the feed leave as it is. `None` until
    /// one asks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<UpdateAttempt>,
    #[serde(default)]
    pub rev: Rev,
}

/// How an update a client asked for is going.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateAttempt {
    /// Downloading, checking and placing `to`, after which the hub restarts on it.
    UnderWay { to: String },
    /// Updating to `to` failed, for `why`, which left the hub as it was, to try again.
    Failed { to: String, why: String },
    /// One a newer hub says, which a client shows as none.
    #[serde(other)]
    Unknown,
}
