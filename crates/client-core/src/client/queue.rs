//! The calls for a session's queue: prompts waiting for the agent to finish.

use protocol::client::ClientRequest;
use protocol::model::{ClientMessageId, Session, SessionId};

use super::{Client, Result};

impl Client {
    /// See [`ClientRequest::SessionQueue`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_queue(&self, id: &SessionId, client_msg_id: ClientMessageId, text: &str) -> Result<Session> {
        self.expect_session(ClientRequest::SessionQueue { id: id.clone(), client_msg_id, text: text.to_string() })
    }

    /// See [`ClientRequest::SessionUnqueue`].
    pub fn session_unqueue(&self, id: &SessionId, client_msg_id: ClientMessageId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionUnqueue { id: id.clone(), client_msg_id })
    }

    /// See [`ClientRequest::SessionSendQueued`].
    pub fn session_send_queued(&self, id: &SessionId, client_msg_id: ClientMessageId) -> Result<Session> {
        self.expect_session(ClientRequest::SessionSendQueued { id: id.clone(), client_msg_id })
    }
}
