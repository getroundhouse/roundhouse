//! What a hub installed as a service last found in the update feed.

use serde::{Deserialize, Serialize};

use super::Rev;

/// The release a hub installed as a service would update to, as it last found it in the update feed, which it reads
/// as it starts and every hour after, as `docs/spec/updates.md` says. A hub has at most one. One that doesn't read the
/// feed, such as one a client started, has none, so a client can tell "not known" from "nothing newer". The hub keeps it
/// in memory, and reads the feed again as it starts. *Unconfirmed:* the shape.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubUpdate {
    /// The newest release whose hub speaks every worker at work, and that has an archive of its hub, if one is newer
    /// than this hub. `None` when none is.
    pub version: Option<String>,
    /// When the hub last read the feed, or tried to, by the hub's clock, in milliseconds since the Unix epoch.
    pub looked_at_ms: i64,
    /// Why the last read failed, such as a feed that couldn't be reached, or whose signature didn't match, which leaves
    /// `version` as the read before found it. `None` once a read worked.
    #[serde(default)]
    pub problem: Option<String>,
    #[serde(default)]
    pub rev: Rev,
}
