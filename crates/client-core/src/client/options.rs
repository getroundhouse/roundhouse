//! The calls that set what a session runs with.

use protocol::capability::settings::Settings;
use protocol::client::sessions;
use protocol::model::{AgentKind, ClientRequestId, Session, SessionId, WorkspaceId};

use super::{Connection, Result};

impl Connection {
    /// See [`sessions::SessionCreate`]. Sending the same `client_request_id` again returns the session the first
    /// request made.
    pub fn session_create(
        &self,
        workspace_id: &WorkspaceId,
        agent: AgentKind,
        settings: Settings,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session> {
        self.call(sessions::SessionCreate { workspace_id: workspace_id.clone(), agent, settings, client_request_id })
    }

    /// See [`sessions::SessionConfigure`].
    pub fn session_configure(&self, id: &SessionId, change: Settings) -> Result<Session> {
        self.call(sessions::SessionConfigure { id: id.clone(), settings: change })
    }
}
