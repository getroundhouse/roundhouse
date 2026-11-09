//! What a workspace's chats say about it, for every client to show the same way.

use protocol::model::{Session, SessionStatus};

/// The open chat that most needs attention speaks for the workspace.
pub fn workspace_status(sessions: &[Session]) -> Option<SessionStatus> {
    let urgency = |status: &SessionStatus| match status {
        SessionStatus::WaitingApproval => 3,
        SessionStatus::Running => 2,
        SessionStatus::Interrupted => 1,
        SessionStatus::Idle | SessionStatus::Unknown => 0,
    };
    sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.status).max_by_key(urgency)
}

/// The chat a workspace opens on: the open one last worked in.
pub fn latest_open_session(sessions: &[Session]) -> Option<&Session> {
    sessions.iter().filter(|s| s.closed_at_ms.is_none()).max_by_key(|s| (s.updated_at_ms, s.created_at_ms))
}

#[cfg(test)]
mod tests {
    use protocol::model::{AgentKind, Session, SessionStatus};

    use super::{latest_open_session, workspace_status};

    fn session(id: &str, status: SessionStatus, closed: bool, updated_at_ms: i64) -> Session {
        Session {
            id: id.into(),
            workspace_id: "w".into(),
            agent: AgentKind::Claude,
            model: None,
            permission_mode: None,
            status,
            last_seq: 0,
            created_at_ms: 0,
            updated_at_ms,
            title: None,
            closed_at_ms: closed.then_some(1),
            rev: protocol::model::Rev::default(),
        }
    }

    #[test]
    fn the_chat_needing_attention_speaks_for_the_workspace_wherever_it_is() {
        use SessionStatus::*;
        let chats =
            [session("a", WaitingApproval, false, 0), session("b", Running, false, 0), session("c", Idle, false, 0)];
        assert_eq!(workspace_status(&chats), Some(WaitingApproval));
        let chats =
            [session("a", Idle, false, 0), session("b", Running, false, 0), session("c", Interrupted, false, 0)];
        assert_eq!(workspace_status(&chats), Some(Running));
    }

    #[test]
    fn a_closed_chat_doesnt_count() {
        use SessionStatus::*;
        assert_eq!(workspace_status(&[session("a", Running, true, 0), session("b", Idle, false, 0)]), Some(Idle));
        assert_eq!(workspace_status(&[session("a", Running, true, 0)]), None);
        let chats = [session("old", Idle, false, 1), session("closed", Idle, true, 9)];
        assert_eq!(latest_open_session(&chats).map(|s| s.id.as_str()), Some("old"));
    }

    #[test]
    fn a_workspace_opens_on_the_chat_last_worked_in() {
        let chats = [session("a", SessionStatus::Idle, false, 5), session("b", SessionStatus::Idle, false, 7)];
        assert_eq!(latest_open_session(&chats).map(|s| s.id.as_str()), Some("b"));
    }
}
