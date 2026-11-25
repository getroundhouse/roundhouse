//! What many lines of the log share, as a span around the work they're from, so a line says what it's about wherever in
//! that work it's from.
//!
//! A task has only the spans it's given, so each begun for something has one of these, or the one it's begun in. Each
//! starts afresh, rather than in the span it's made in, since a session or a worker's link that a request brings about
//! outlasts the request. They're at `error`, since one below the level a log has isn't there for the lines in it.

use protocol::model::{InstanceId, SessionId, WorkspaceId};
use tracing::{Span, error_span};

use crate::session::LiveSession;

/// A client's request: the connection it came on, and its call's ID there, which another connection's can have too.
pub(crate) fn request(connection: u64, call: u64) -> Span {
    error_span!(parent: None, "request", connection, call)
}

pub(crate) fn session(workspace_id: &WorkspaceId, session_id: &SessionId) -> Span {
    error_span!(parent: None, "session", %workspace_id, %session_id)
}

impl LiveSession {
    /// What the log has of the session, for a task begun for it.
    pub(crate) fn span(&self) -> Span {
        session(&self.workspace_id, &self.id)
    }
}

pub(crate) fn workspace(workspace_id: &WorkspaceId) -> Span {
    error_span!(parent: None, "workspace", %workspace_id)
}

/// The hub's link to a workspace's worker.
pub(crate) fn worker(workspace_id: &WorkspaceId, instance_id: &InstanceId) -> Span {
    error_span!(parent: None, "worker", %workspace_id, %instance_id)
}

#[cfg(test)]
mod tests {
    use logging::testing::Logs;
    use serde_json::json;
    use tracing::warn;

    #[test]
    fn a_span_made_in_another_has_nothing_of_it() {
        let logs = Logs::capture();

        super::request(3, 7).in_scope(|| {
            warn!(name: "a.request", "In the request");
            super::session(&"w".into(), &"s".into()).in_scope(|| warn!(name: "a.session", "In the session"));
        });

        let contexts: Vec<_> = logs.events().into_iter().map(|event| event["context"].clone()).collect();
        assert_eq!(
            contexts,
            [json!({ "connection": 3, "call": 7 }), json!({ "workspace_id": "w", "session_id": "s" })]
        );
    }
}
