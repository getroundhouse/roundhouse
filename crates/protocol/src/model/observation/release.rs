//! What the hub observes of the releases it could update to.

use serde::{Deserialize, Serialize};

/// What the update feed offers a hub installed as a service.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReleaseOffer {
    /// The newest release whose hub speaks every worker at work, and that has an archive of its hub, if one is newer
    /// than this hub. `None` when none is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
