//! What's in the new-workspace dialog and not made yet, and the files waiting with it, which the hub keeps before any
//! workspace or chat exists to add them to. What each file is goes in the database, and its bytes in
//! `attachment_files`, as a chat's do.

use std::time::Duration;

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, file_name};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AttachmentId, NewWorkspaceDraft, NewWorkspaceDraftToKeep};
use tracing::warn;

use super::Hub;
use super::attachments::checked;
use crate::store::{Store, now_ms};

#[cfg(test)]
mod tests;

/// How long a file added to the dialog waits for a draft to name it. A client adds one just before keeping the draft
/// that names it, so one nothing names after this never will be.
const UNNAMED_FOR: Duration = Duration::from_hours(24);

impl Hub {
    /// Keeps `draft`, and tells every client following the feed when that changed it.
    pub(crate) async fn new_workspace_draft_keep(
        &self,
        draft: NewWorkspaceDraftToKeep,
    ) -> Result<NewWorkspaceDraft, ApiError> {
        let (kept, changed) = self.blocking(move |store| store.set_new_workspace_draft(&draft)).await?;
        if changed {
            self.notify(HubNotice::NewWorkspaceDraft(kept.clone()));
            self.let_go_of_unnamed_draft_files().await;
        }
        Ok(kept)
    }

    /// Lets go of the dialog's files that nothing has named for [`UNNAMED_FOR`], and of their bytes, such as one a
    /// client took out without the hub hearing. One left behind only takes room, so a failure is logged.
    async fn let_go_of_unnamed_draft_files(&self) {
        let before = now_ms().saturating_sub(i64::try_from(UNNAMED_FOR.as_millis()).unwrap_or(i64::MAX));
        let unnamed = self.blocking(move |store| store.let_go_of_unnamed_new_workspace_draft_files(before)).await;
        let unnamed = unnamed.unwrap_or_else(|e| {
            warn!("couldn't let go of the new-workspace dialog's files nothing names: {e}");
            Vec::new()
        });
        for id in unnamed {
            if let Err(e) = self.attachment_files.remove_draft_file(&id).await {
                warn!(attachment = %id, "couldn't let go of a file nothing will send: {e}");
            }
        }
    }

    /// As the hub starts, before it serves anything, lets go of the dialog's files nothing has named for long, and of
    /// bytes on disk with no file of the dialog's, which nothing could ask for.
    pub(crate) async fn tidy_draft_files(&self) {
        self.let_go_of_unnamed_draft_files().await;
        let tidied = match self.blocking(Store::new_workspace_draft_file_ids).await {
            Ok(kept) => self.attachment_files.remove_draft_files_except(kept).await,
            Err(e) => Err(e),
        };
        if let Err(e) = tidied {
            warn!("couldn't tidy the new-workspace dialog's files: {e}");
        }
    }

    /// Sent again with the same ID, it returns what the first one added without looking at the rest, as a chat's file
    /// does.
    pub(crate) async fn new_workspace_draft_file_add(
        &self,
        id: AttachmentId,
        name: String,
        media_type: MediaType,
        data: AttachmentBytes,
    ) -> Result<Attachment, ApiError> {
        self.attachment_files.check_draft_file(&id)?;
        let earlier = {
            let id = id.clone();
            self.blocking(move |store| store.new_workspace_draft_file(&id)).await?
        };
        if let Some(earlier) = earlier {
            return Ok(earlier);
        }
        checked(media_type, &data)?;
        let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        let file = Attachment { id: id.clone(), name: file_name(&name), media_type, size_bytes };
        self.attachment_files.write_draft_file(&id, data.into_vec()).await?;
        let added = self.blocking(move |store| store.add_new_workspace_draft_file(&file)).await;
        if added.is_err() {
            let _ = self.attachment_files.remove_draft_file(&id).await;
        }
        added
    }

    pub(crate) async fn new_workspace_draft_file_read(&self, id: AttachmentId) -> Result<AttachmentBytes, ApiError> {
        let file = {
            let id = id.clone();
            self.blocking(move |store| store.new_workspace_draft_file(&id)).await?
        };
        let Some(file) = file else {
            return Err(ApiError::new(ErrorCode::NotFound, format!("no attachment {id}")));
        };
        match self.attachment_files.read_draft_file(&id).await? {
            Some(bytes) => Ok(AttachmentBytes::new(bytes)),
            None => Err(ApiError::new(
                ErrorCode::NotFound,
                format!("{} can't be read: the hub no longer has it", file.name),
            )),
        }
    }

    /// Lets go of a file the draft doesn't name, and of its bytes. One whose bytes can't be let go of is only left on
    /// disk, so it's logged.
    pub(crate) async fn new_workspace_draft_file_remove(&self, id: AttachmentId) -> Result<(), ApiError> {
        self.attachment_files.check_draft_file(&id)?;
        let removed = {
            let id = id.clone();
            self.blocking(move |store| store.let_go_of_new_workspace_draft_file(&id)).await?
        };
        if removed && let Err(e) = self.attachment_files.remove_draft_file(&id).await {
            warn!(attachment = %id, "couldn't let go of a file nothing will send: {e}");
        }
        Ok(())
    }
}
