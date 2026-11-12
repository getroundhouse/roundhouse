//! The calls for a session's queue: prompts waiting for the agent to finish.

use protocol::client::{ClientRequest, queue};
use protocol::model::{AttachmentId, ClientMessageId, Session, SessionId};
use protocol::prompt::Prompt;

use super::{Client, Result};

impl Client {
    /// See [`ClientRequest::SessionQueue`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_queue(&self, id: &SessionId, client_msg_id: ClientMessageId, text: &str) -> Result<Session> {
        self.session_queue_with(id, client_msg_id, text, Vec::new())
    }

    /// A queued prompt that carries files, each added first with [`Client::attachment_add`]. With any, `text` can be
    /// empty. [`Client::queue_prompt_carrying`] adds them and queues it in its place in its chat's line.
    pub fn session_queue_with(
        &self,
        id: &SessionId,
        client_msg_id: ClientMessageId,
        text: &str,
        attachments: Vec<AttachmentId>,
    ) -> Result<Session> {
        let text = text.to_string();
        self.expect_session(ClientRequest::SessionQueue(queue::SessionQueue {
            id: id.clone(),
            client_msg_id,
            prompt: Prompt::carrying(text, attachments),
        }))
    }

    /// See [`ClientRequest::SessionChangeQueued`].
    pub fn session_change_queued(&self, id: &SessionId, client_msg_id: ClientMessageId, text: &str) -> Result<Session> {
        let text = text.to_string();
        self.expect_session(ClientRequest::SessionChangeQueued(queue::SessionChangeQueued {
            id: id.clone(),
            client_msg_id,
            text,
        }))
    }

    /// See [`ClientRequest::SessionUnqueue`].
    pub fn session_unqueue(&self, id: &SessionId, client_msg_id: ClientMessageId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionUnqueue(queue::SessionUnqueue { id: id.clone(), client_msg_id }))
    }

    /// See [`ClientRequest::SessionSendQueued`].
    pub fn session_send_queued(&self, id: &SessionId, client_msg_id: ClientMessageId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionSendQueued(queue::SessionSendQueued {
            id: id.clone(),
            client_msg_id,
        }))
    }
}
