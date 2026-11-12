//! What's in the new-workspace dialog and not made yet, and the files waiting with it, which the hub keeps before any
//! workspace or chat exists to add them to. What each file is goes in the database, and its bytes in
//! `attachment_files`, as a chat's do.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, file_name};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AttachmentId, WorkspaceDraft, WorkspaceDraftToKeep};
use tracing::warn;

use super::Hub;
use super::attachments::checked;

impl Hub {
    /// Keeps `draft`, and tells every client following the feed when that changed it.
    pub(crate) async fn workspace_draft_keep(&self, draft: WorkspaceDraftToKeep) -> Result<WorkspaceDraft, ApiError> {
        let (kept, changed) = self.blocking(move |store| store.set_workspace_draft(&draft)).await?;
        if changed {
            self.notify(HubNotice::WorkspaceDraft(kept.clone()));
        }
        Ok(kept)
    }

    /// Sent again with the same ID, it returns what the first one added without looking at the rest, as a chat's file
    /// does.
    pub(crate) async fn workspace_draft_file_add(
        &self,
        id: AttachmentId,
        name: String,
        media_type: MediaType,
        data: AttachmentBytes,
    ) -> Result<Attachment, ApiError> {
        self.attachment_files.check_draft_file(&id)?;
        let earlier = {
            let id = id.clone();
            self.blocking(move |store| store.workspace_draft_file(&id)).await?
        };
        if let Some(earlier) = earlier {
            return Ok(earlier);
        }
        checked(media_type, &data)?;
        let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        let file = Attachment { id: id.clone(), name: file_name(&name), media_type, size_bytes };
        self.attachment_files.write_draft_file(&id, data.into_vec()).await?;
        let added = self.blocking(move |store| store.add_workspace_draft_file(&file)).await;
        if added.is_err() {
            let _ = self.attachment_files.remove_draft_file(&id).await;
        }
        added
    }

    pub(crate) async fn workspace_draft_file_read(&self, id: AttachmentId) -> Result<AttachmentBytes, ApiError> {
        let file = {
            let id = id.clone();
            self.blocking(move |store| store.workspace_draft_file(&id)).await?
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
    pub(crate) async fn workspace_draft_file_remove(&self, id: AttachmentId) -> Result<(), ApiError> {
        self.attachment_files.check_draft_file(&id)?;
        let removed = {
            let id = id.clone();
            self.blocking(move |store| store.let_go_of_workspace_draft_file(&id)).await?
        };
        if removed && let Err(e) = self.attachment_files.remove_draft_file(&id).await {
            warn!(attachment = %id, "couldn't let go of a file nothing will send: {e}");
        }
        Ok(())
    }
}
