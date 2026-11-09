//! The calls that set what a session runs with. Each checks the hub took every setting, since one from before a setting
//! ignores it rather than refusing.

use protocol::client::{ApiError, ClientRequest, ErrorCode};
use protocol::model::{AgentKind, ClientRequestId, Session, SessionId, SessionOptions, WorkspaceId};

use super::{Client, ClientError, Result};

impl Client {
    /// See [`ClientRequest::SessionCreate`]. Sending the same `client_request_id` again returns the session the first
    /// request made. A hub from before effort makes the session without one, and this then fails with
    /// [`ErrorCode::Unsupported`], though the session is there.
    pub fn session_create(
        &self,
        workspace_id: &WorkspaceId,
        agent: AgentKind,
        options: SessionOptions,
        client_request_id: Option<ClientRequestId>,
    ) -> Result<Session> {
        let asked = options.clone();
        let request =
            ClientRequest::SessionCreate { workspace_id: workspace_id.clone(), agent, options, client_request_id };
        as_asked(&asked, self.expect_session(request)?)
    }

    /// See [`ClientRequest::SessionConfigure`]. A hub from before effort ignores it, and this then fails with
    /// [`ErrorCode::Unsupported`], though the hub took the other settings.
    pub fn session_configure(&self, id: &SessionId, options: SessionOptions) -> Result<Session> {
        let asked = options.clone();
        as_asked(&asked, self.expect_session(ClientRequest::SessionConfigure { id: id.clone(), options })?)
    }
}

/// The session, unless the hub left out a setting it was asked for, which only one older than this client does.
fn as_asked(asked: &SessionOptions, session: Session) -> Result<Session> {
    if asked.effort.is_some() && session.effort.is_none() {
        let message = "the hub is older than the app, and can't set the effort until it's updated";
        return Err(ClientError::Api(ApiError::new(ErrorCode::Unsupported, message)));
    }
    Ok(session)
}

#[cfg(test)]
mod tests;
