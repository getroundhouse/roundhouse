//! The calls that set what a session runs with.

use protocol::client::sessions;
use protocol::model::{AgentKind, ClientRequestId, Session, SessionId, SessionSettingsChange, WorkspaceId};

use super::{Client, Result};

impl Client {
    /// See [`sessions::SessionCreate`]. Sending the same `client_request_id` again returns the session the first
    /// request made.
    pub fn session_create(
        &self,
        workspace_id: &WorkspaceId,
        agent: AgentKind,
        options: SessionSettingsChange,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session> {
        self.call(sessions::SessionCreate { workspace_id: workspace_id.clone(), agent, options, client_request_id })
    }

    /// See [`sessions::SessionConfigure`].
    pub fn session_configure(&self, id: &SessionId, options: SessionSettingsChange) -> Result<Session> {
        self.call(sessions::SessionConfigure { id: id.clone(), options })
    }
}
