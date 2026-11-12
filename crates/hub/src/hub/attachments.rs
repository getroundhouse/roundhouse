//! Files a person adds to a session, for its prompts to carry, or to the new-workspace dialog, before any session
//! exists, and images a tool sends its agent. Each is added, read and let go of the same way, whatever it's for. What
//! each is goes in the database, and its bytes in `attachment_files`. Before a prompt reaches its agent, the worker
//! puts each of its files in the worktree, and then the hub keeps the bytes only of an image, since that's all the
//! transcript draws.

use std::sync::Arc;

use protocol::attachment::{
    Attachment, AttachmentBytes, AttachmentFile, AttachmentProblem, MediaType, NewAttachment, file_name,
};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AttachmentId, SessionId, WorkspaceId};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tracing::warn;

use super::Hub;
use crate::attachment_files::FileFor;
use crate::error::internal;
use crate::store::AddedBy;
use crate::worker_link::WorkerLink;

/// Refuses a file that isn't what it says, or that's empty or too large. A kind from a newer client is refused as
/// unsupported, so it can fall back to sending the file as `Other`.
pub(super) fn checked(media_type: MediaType, data: &AttachmentBytes) -> Result<(), ApiError> {
    let Err(problem) = media_type.check(data.as_slice()) else { return Ok(()) };
    let code = match problem {
        AttachmentProblem::Unsupported => ErrorCode::Unsupported,
        AttachmentProblem::Empty | AttachmentProblem::TooLarge { .. } | AttachmentProblem::NotWhatItSays { .. } => {
            ErrorCode::InvalidRequest
        }
    };
    Err(ApiError::new(code, problem.to_string()))
}

impl Hub {
    /// Keeps a file for what it's for. Sent again with the same ID, it returns what the first one added without looking
    /// at the rest, the way a prompt is recorded once.
    pub(crate) async fn file_add(
        &self,
        kept_for: FileFor,
        id: AttachmentId,
        name: String,
        media_type: MediaType,
        data: AttachmentBytes,
        by: AddedBy,
    ) -> Result<Attachment, ApiError> {
        self.attachment_files.check(&kept_for, &id)?;
        let earlier = {
            let (kept_for, id) = (kept_for.clone(), id.clone());
            self.blocking(move |store| {
                if let FileFor::Session(session_id) = &kept_for {
                    store.get_session(session_id)?;
                }
                store.file(&kept_for, &id)
            })
            .await?
        };
        if let Some(earlier) = earlier {
            return Ok(earlier);
        }
        checked(media_type, &data)?;
        let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        let file = Attachment { id: id.clone(), name: file_name(&name), media_type, size_bytes };
        self.attachment_files.write(&kept_for, &id, data.into_vec()).await?;
        let added = {
            let kept_for = kept_for.clone();
            self.blocking(move |store| store.add_file(&kept_for, &file, by)).await
        };
        if added.is_err() {
            // Such as because its session was deleted meanwhile, which takes the session's files with it.
            let _ = self.attachment_files.remove(&kept_for, &id).await;
        }
        added
    }

    /// Keeps the images an event from the agent names, before the event is recorded, so a client never asks for one
    /// before the hub has it. Each is added as a client's file is, with the same checks, and only an image is kept. One
    /// that can't be is logged, and the transcript says it couldn't show it.
    pub(crate) async fn keep_from_agent(&self, session_id: &SessionId, attachments: Vec<NewAttachment>) {
        for NewAttachment { attachment: Attachment { id, name, media_type, .. }, data } in attachments {
            if !media_type.is_image() {
                warn!(session = %session_id, attachment = %id, "the agent sent {} as an image", media_type.mime());
                continue;
            }
            let kept_for = FileFor::Session(session_id.clone());
            let added = self.file_add(kept_for, id.clone(), name, media_type, data, AddedBy::Agent);
            if let Err(e) = added.await {
                warn!(session = %session_id, attachment = %id, "couldn't keep an image from the agent: {e}");
            }
        }
    }

    /// A file's bytes, such as an image's, for drawing it, or one of the new-workspace dialog's, to send with the first
    /// prompt. A session's other files' are gone once a prompt has put them in the worktree.
    pub(crate) async fn file_read(&self, kept_for: FileFor, id: AttachmentId) -> Result<AttachmentBytes, ApiError> {
        let file = {
            let (kept_for, id) = (kept_for.clone(), id.clone());
            self.blocking(move |store| store.file(&kept_for, &id)).await?
        };
        let Some(file) = file else {
            return Err(ApiError::new(ErrorCode::NotFound, format!("no attachment {id}")));
        };
        if let Some(bytes) = self.attachment_files.read(&kept_for, &id).await? {
            return Ok(AttachmentBytes::new(bytes));
        }
        let why = if file.media_type.is_image() || kept_for == FileFor::NewWorkspace {
            "the hub no longer has it"
        } else {
            "the hub keeps only images once they've been sent"
        };
        Err(ApiError::new(ErrorCode::NotFound, format!("{} can't be read: {why}", file.name)))
    }

    /// Lets go of a file a client added that nothing will send, as [`crate::store::Store::let_go_of_file`] decides,
    /// and of its bytes.
    pub(crate) async fn file_remove(&self, kept_for: FileFor, id: AttachmentId) -> Result<(), ApiError> {
        self.attachment_files.check(&kept_for, &id)?;
        let removed = {
            let (kept_for, id) = (kept_for.clone(), id.clone());
            self.blocking(move |store| store.let_go_of_file(&kept_for, &id)).await?
        };
        if removed {
            self.let_go_of_bytes(&kept_for, &[id]).await;
        }
        Ok(())
    }

    /// The bytes of files the store has let go of. One that can't be is only left on disk, so it's logged.
    pub(crate) async fn let_go_of_bytes(&self, kept_for: &FileFor, files: &[AttachmentId]) {
        for id in files {
            if let Err(e) = self.attachment_files.remove(kept_for, id).await {
                warn!(?kept_for, attachment = %id, "couldn't let go of a file nothing will send: {e}");
            }
        }
    }

    /// Has the worker put each file in the worktree, unless a prompt already has, and lets go of the hub's copy of any
    /// that isn't an image. Where each went is kept before the copy goes, so failing part way only leaves a copy behind.
    pub(crate) async fn place_attachments(
        self: &Arc<Self>,
        session_id: &SessionId,
        workspace_id: &WorkspaceId,
        attachments: &[Attachment],
    ) -> Result<Vec<AttachmentFile>, ApiError> {
        if attachments.is_empty() {
            return Ok(Vec::new());
        }
        let ready = self.ready_worktree(workspace_id).await?;
        let (worktree, link) = (ready.path(), ready.link());
        let mut placed = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            let (session, id) = (session_id.clone(), attachment.id.clone());
            let already = self.blocking(move |store| store.attachment_placed_at(&session, &id)).await?;
            let path = match already {
                Some(path) => path,
                None => self.place(session_id, link, worktree, attachment).await?,
            };
            placed.push(AttachmentFile { attachment: attachment.clone(), path });
        }
        Ok(placed)
    }

    /// Returns where the worker put it.
    async fn place(
        &self,
        session_id: &SessionId,
        link: &WorkerLink,
        worktree: &str,
        attachment: &Attachment,
    ) -> Result<String, ApiError> {
        let kept_for = FileFor::Session(session_id.clone());
        let Some(data) = self.attachment_files.read(&kept_for, &attachment.id).await? else {
            return Err(internal(format!("the hub doesn't have {} to send", attachment.name)));
        };
        let (path, name, data) = (worktree.to_string(), attachment.name.clone(), AttachmentBytes::new(data));
        let reply = link.call(WorkerRequest::AttachmentWrite { path, name, data }).await?;
        let WorkerResponse::AttachmentWritten { path } = reply else {
            return Err(internal("the worker didn't say where it put a file"));
        };
        let (session, id, kept) = (session_id.clone(), attachment.id.clone(), path.clone());
        self.blocking(move |store| store.set_attachment_placed_at(&session, &id, &kept)).await?;
        if !attachment.media_type.is_image()
            && let Err(e) = self.attachment_files.remove(&kept_for, &attachment.id).await
        {
            warn!(attachment = %attachment.id, "couldn't let go of a sent attachment: {e}");
        }
        Ok(path)
    }
}
