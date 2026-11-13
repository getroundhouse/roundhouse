use super::{CommentId, Rev, WorkspaceId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Old,
    New,
    #[serde(other)]
    Unknown,
}

/// A review comment on a changed line, waiting to be sent to the agent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DiffComment {
    pub id: CommentId,
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    pub text: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}
