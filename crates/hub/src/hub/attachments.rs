//! Files a person adds to a session, for its prompts to carry, and images a tool sends its agent. What each is goes in
//! the database, and its bytes in `attachment_files`. Before a prompt reaches its agent, the worker puts each of its
//! files in the worktree, and then the hub keeps the bytes only of an image, since that's all the transcript draws.

use std::sync::Arc;

use protocol::attachment::{
    Attachment, AttachmentBytes, AttachmentFile, AttachmentProblem, MediaType, NewAttachment, file_name,
};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AttachmentId, SessionId, WorkspaceId};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tracing::warn;

use super::Hub;
use crate::error::internal;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Sent again with the same ID, it returns what the first one added without looking at the rest, the way a prompt
    /// is recorded once.
    pub(crate) async fn attachment_add(
        &self,
        session_id: SessionId,
        id: AttachmentId,
        name: String,
        media_type: MediaType,
        data: AttachmentBytes,
    ) -> Result<Attachment, ApiError> {
        self.attachment_files.check(&session_id, &id)?;
        let earlier = {
            let (session_id, id) = (session_id.clone(), id.clone());
            self.blocking(move |store| {
                store.get_session(&session_id)?;
                store.attachment(&session_id, &id)
            })
            .await?
        };
        if let Some(earlier) = earlier {
            return Ok(earlier);
        }
        if let Err(problem) = media_type.check(data.as_slice()) {
            // A kind from a newer client is refused as unsupported, so it can fall back to sending the file as `Other`.
            let code = match problem {
                AttachmentProblem::Unsupported => ErrorCode::Unsupported,
                AttachmentProblem::Empty
                | AttachmentProblem::TooLarge { .. }
                | AttachmentProblem::NotWhatItSays { .. } => ErrorCode::InvalidRequest,
            };
            return Err(ApiError::new(code, problem.to_string()));
        }
        let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        let attachment = Attachment { id: id.clone(), name: file_name(&name), media_type, size_bytes };
        self.attachment_files.write(&session_id, &id, data.into_vec()).await?;
        self.blocking(move |store| store.add_attachment(&session_id, &attachment)).await
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
            if let Err(e) = self.attachment_add(session_id.clone(), id.clone(), name, media_type, data).await {
                warn!(session = %session_id, attachment = %id, "couldn't keep an image from the agent: {e}");
            }
        }
    }

    /// An image's bytes, for drawing it. Any other file's are gone once a prompt has put it in the worktree.
    pub(crate) async fn attachment_read(
        &self,
        session_id: SessionId,
        id: AttachmentId,
    ) -> Result<AttachmentBytes, ApiError> {
        let attachment = {
            let (session_id, id) = (session_id.clone(), id.clone());
            self.blocking(move |store| store.attachment(&session_id, &id)).await?
        };
        let Some(attachment) = attachment else {
            return Err(ApiError::new(ErrorCode::NotFound, format!("no attachment {id}")));
        };
        if let Some(bytes) = self.attachment_files.read(&session_id, &id).await? {
            return Ok(AttachmentBytes::new(bytes));
        }
        let why = if attachment.media_type.is_image() {
            "the hub no longer has it"
        } else {
            "the hub keeps only images once they've been sent"
        };
        Err(ApiError::new(ErrorCode::NotFound, format!("{} can't be read: {why}", attachment.name)))
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
        let worktree = self.workspace(workspace_id).await?.path;
        let worktree =
            worktree.ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the workspace has no worktree yet"))?;
        let link = self.ensure_worker(workspace_id).await?;
        let mut placed = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            let (session, id) = (session_id.clone(), attachment.id.clone());
            let already = self.blocking(move |store| store.attachment_placed_at(&session, &id)).await?;
            let path = match already {
                Some(path) => path,
                None => self.place(session_id, &link, &worktree, attachment).await?,
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
        let Some(data) = self.attachment_files.read(session_id, &attachment.id).await? else {
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
            && let Err(e) = self.attachment_files.remove(session_id, &attachment.id).await
        {
            warn!(attachment = %attachment.id, "couldn't let go of a sent attachment: {e}");
        }
        Ok(path)
    }
}
