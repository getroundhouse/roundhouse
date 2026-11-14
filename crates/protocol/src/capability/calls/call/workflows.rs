//! Running a workflow.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Workflow {
    /// What the agent calls it, when it names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
