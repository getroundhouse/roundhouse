//! The requests for a session's queue: prompts waiting for the agent to finish, which the live session keeps.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::model::{AttachmentId, ClientMessageId, Session, SessionId};
use protocol::prompt::check_prompt;
use tokio::sync::oneshot;

use super::Hub;
use crate::error::internal;
use crate::session::Command;
use crate::store::Store;

impl Hub {
    pub(crate) async fn session_queue(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
        attachments: Vec<AttachmentId>,
    ) -> Result<Session, ApiError> {
        check_prompt(&text, attachments.len())?;
        self.live_session(&id).await?.queue(client_msg_id, text, attachments).await
    }

    pub(crate) async fn session_change_queued(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
    ) -> Result<Session, ApiError> {
        self.live_session(&id).await?.change_queued(client_msg_id, text).await
    }

    pub(crate) async fn session_unqueue(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
    ) -> Result<Session, ApiError> {
        self.live_session(&id).await?.unqueue(client_msg_id).await
    }

    pub(crate) async fn session_send_queued(
        self: &Arc<Self>,
        id: SessionId,
        client_msg_id: ClientMessageId,
    ) -> Result<Session, ApiError> {
        let session = self.live_session(&id).await?;
        let (reply, sent) = oneshot::channel();
        session.command(Command::SendQueuedNow { client_msg_id, reply }).await?;
        sent.await.map_err(|_| internal("the session has stopped"))?
    }

    /// At startup, starts each session with a queue, so a queue that was ready to send when the last hub went away is
    /// sent now, rather than once someone opens its chat.
    pub(crate) async fn start_queues(self: &Arc<Self>) {
        let ids = match self.blocking(Store::sessions_with_queues).await {
            Ok(ids) => ids,
            Err(e) => return tracing::error!("couldn't read the queues: {e}"),
        };
        for id in ids {
            if let Err(e) = self.live_session(&id).await {
                tracing::error!(session = %id, "couldn't start a session with a queue: {e}");
            }
        }
    }
}
