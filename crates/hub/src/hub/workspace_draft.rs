//! What's in the new-workspace dialog and not made yet, and the files waiting with it, which the hub keeps before any
//! workspace or chat exists to add them to. Its files are added, read and let go of as a chat's are, by `attachments`,
//! and only letting go of those nothing names is the dialog's own.

use std::time::Duration;

use protocol::client::{ApiError, HubNotice};
use protocol::model::{WorkspaceDraft, WorkspaceDraftToKeep};
use tracing::warn;

use super::Hub;
use crate::attachment_files::FileFor;
use crate::store::{Store, now_ms};

#[cfg(test)]
mod tests;

/// How long a file added to the dialog waits for a draft to name it. A client adds one just before keeping the draft
/// that names it, so one nothing names after this never will be.
const UNNAMED_FOR: Duration = Duration::from_hours(24);

impl Hub {
    /// Keeps `draft`, and tells every client following the feed when that changed it.
    pub(crate) async fn workspace_draft_keep(&self, draft: WorkspaceDraftToKeep) -> Result<WorkspaceDraft, ApiError> {
        let (kept, changed) = self.blocking(move |store| store.set_workspace_draft(&draft)).await?;
        if changed {
            self.notify(HubNotice::WorkspaceDraft(kept.clone()));
            self.let_go_of_unnamed_draft_files().await;
        }
        Ok(kept)
    }

    /// Lets go of the dialog's files that nothing has named for [`UNNAMED_FOR`], and of their bytes, such as one a
    /// client took out without the hub hearing. One left behind only takes room, so a failure is logged.
    async fn let_go_of_unnamed_draft_files(&self) {
        let before = now_ms().saturating_sub(i64::try_from(UNNAMED_FOR.as_millis()).unwrap_or(i64::MAX));
        let unnamed = self.blocking(move |store| store.let_go_of_unnamed_workspace_draft_files(before)).await;
        let unnamed = unnamed.unwrap_or_else(|e| {
            warn!("couldn't let go of the new-workspace dialog's files nothing names: {e}");
            Vec::new()
        });
        self.let_go_of_bytes(&FileFor::NewWorkspace, &unnamed).await;
    }

    /// As the hub starts, before it serves anything, lets go of the dialog's files nothing has named for long, and of
    /// bytes on disk with no file of the dialog's, which nothing could ask for.
    pub(crate) async fn tidy_draft_files(&self) {
        self.let_go_of_unnamed_draft_files().await;
        let tidied = match self.blocking(Store::workspace_draft_file_ids).await {
            Ok(kept) => self.attachment_files.remove_draft_files_except(kept).await,
            Err(e) => Err(e),
        };
        if let Err(e) = tidied {
            warn!("couldn't tidy the new-workspace dialog's files: {e}");
        }
    }
}
