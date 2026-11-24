//! What the hub observes of a workspace that the workspace's record doesn't hold.

use serde::{Deserialize, Serialize};

/// That a workspace's worker is another build than its hub's, such as one an earlier hub left running, as after an
/// update, which the hub carries on with until nothing in the workspace is at work, and then replaces with its own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OtherBuild {
    /// The build, as the worker's hello named it. `None` where its hello named none, which is another build all the
    /// same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}
