//! The calls that give a session's agent prompts, sent now or queued for when it's finished, and change its queue.
//! Clients send and queue prompts with [`crate::OpenSession::start_sending`], [`Client::start_queueing`] and
//! [`Client::send_prompt`], which add their files first, in their place in their chat's line. The calls here that
//! send one whose files are added already are for tests.

use protocol::client::{ClientRequest, ClientResponse, queue};
use protocol::model::{AttachmentId, ClientMessageId, Seq, Session, SessionId};
use protocol::prompt::Prompt;

use super::{Client, Result, unexpected};

impl Client {
    /// Sends a prompt that says only `text`, for a test, which can send it under no ID, as a client from before them
    /// did. Returns its `seq`, and whether it had already been recorded.
    #[cfg(any(test, feature = "testing"))]
    pub fn session_prompt(
        &self,
        id: &SessionId,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
    ) -> Result<(Seq, bool)> {
        self.prompt_carried(id, client_msg_id, Prompt::new(text))
    }

    /// The same, for a prompt that can carry files, each added first with [`Client::attachment_add`].
    #[cfg(any(test, feature = "testing"))]
    pub fn session_prompt_with(
        &self,
        id: &SessionId,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
    ) -> Result<(Seq, bool)> {
        self.prompt_carried(id, client_msg_id, prompt)
    }

    /// Sends a prompt whose files have each been added to the session.
    pub(crate) fn prompt_carried(
        &self,
        id: &SessionId,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
    ) -> Result<(Seq, bool)> {
        let reply = self.call(ClientRequest::SessionPrompt { id: id.clone(), client_msg_id, prompt })?;
        let ClientResponse::PromptRecorded { seq, duplicate } = reply else {
            return unexpected(&reply);
        };
        Ok((seq, duplicate))
    }

    /// Queues a prompt that says only `text`, for a test. A hub older than queues refuses it, as `is_unsupported`
    /// says.
    #[cfg(any(test, feature = "testing"))]
    pub fn session_queue(&self, id: &SessionId, client_msg_id: ClientMessageId, text: &str) -> Result<Session> {
        self.queue_carried(id, client_msg_id, Prompt::new(text))
    }

    /// The same, for a prompt that can carry files, each added first with [`Client::attachment_add`].
    #[cfg(any(test, feature = "testing"))]
    pub fn session_queue_with(
        &self,
        id: &SessionId,
        client_msg_id: ClientMessageId,
        prompt: Prompt<AttachmentId>,
    ) -> Result<Session> {
        self.queue_carried(id, client_msg_id, prompt)
    }

    /// Queues a prompt whose files have each been added to the session. See [`ClientRequest::SessionQueue`].
    pub(crate) fn queue_carried(
        &self,
        id: &SessionId,
        client_msg_id: ClientMessageId,
        prompt: Prompt<AttachmentId>,
    ) -> Result<Session> {
        self.expect_session(ClientRequest::SessionQueue(queue::SessionQueue { id: id.clone(), client_msg_id, prompt }))
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
