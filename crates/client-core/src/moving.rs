//! What a workspace says while its record says it's moving: on its way out to the archive, or back from it.

use protocol::model::WorkspaceStatus;

/// What a workspace says in place of its button to archive or restore it while its record's `status` says it's on its
/// way out or back, so every app shows it, wherever it was asked for.
pub fn doing(status: WorkspaceStatus) -> Option<&'static str> {
    match status {
        WorkspaceStatus::Archiving => Some("Archiving…"),
        WorkspaceStatus::Restoring => Some("Restoring…"),
        WorkspaceStatus::Creating | WorkspaceStatus::Ready | WorkspaceStatus::Archived | WorkspaceStatus::Unknown => {
            None
        }
    }
}
