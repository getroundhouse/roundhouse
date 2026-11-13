//! A terminal open in a workspace, as every client lists it.

use serde::{Deserialize, Serialize};

use super::{Rev, TerminalId, WorkspaceId};

/// A terminal open in a workspace. Its shell is its worker's, so the hub keeps this only while the worker says the
/// shell runs, and keeps it in memory rather than storing it, with a revision from the counter every record's comes
/// from. A record of a terminal whose worker is gone would only go stale.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Terminal {
    pub id: TerminalId,
    pub workspace_id: WorkspaceId,
    /// When it was opened, so every client shows a workspace's terminals in the same order.
    pub opened_at_ms: i64,
    pub rev: Rev,
}
