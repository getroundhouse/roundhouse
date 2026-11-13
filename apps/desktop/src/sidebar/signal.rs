//! What a chat's mark says of its agent, which chat's a workspace's mark shows, and whether a chat is waiting on the
//! person, which its name says rather than its mark.

use look::Signal;
use protocol::event::TaskKind;
use protocol::model::{Session, SessionStatus};

/// A workspace's mark. The open chat with the most going on speaks for it. A reply nobody has read isn't on the mark,
/// and neither is a chat waiting on an answer: the name beside it says to look, so that shows along with whatever
/// the agent is doing.
pub(crate) fn workspace(chats: &[Session]) -> Signal {
    let urgency = |signal: &Signal| match signal {
        Signal::Working | Signal::SettingUp => 3,
        Signal::Failed => 2,
        Signal::Background => 1,
        Signal::Idle => 0,
    };
    let open = chats.iter().filter(|chat| chat.is_open());
    open.map(chat).max_by_key(urgency).unwrap_or(Signal::Idle)
}

/// A chat's mark. The light sweeps only for the agent's own work: a turn, or a subagent it left going
/// when its turn ended. Anything else left going might end by itself, as a test run does, or be held open, as an app
/// is, and the agent doesn't say which. So that gets a quieter light, which claims only that something is running.
/// An agent that stopped short, by going away mid-turn or failing its turn, needs the person more than anything it
/// left going does.
pub(crate) fn chat(session: &Session) -> Signal {
    match session.status {
        SessionStatus::Running => Signal::Working,
        SessionStatus::Interrupted => Signal::Failed,
        // Its turn failed, whatever it left going.
        SessionStatus::Idle | SessionStatus::Background if session.turn_failed() => Signal::Failed,
        SessionStatus::Background if session.background_tasks.iter().any(|task| task.kind == TaskKind::Subagent) => {
            Signal::Working
        }
        SessionStatus::Background => Signal::Background,
        // Waiting on an answer or an approval, the agent holds still, and the chat's name says it needs a look.
        SessionStatus::WaitingApproval | SessionStatus::Idle | SessionStatus::Unknown => Signal::Idle,
    }
}

/// Whether an open chat is waiting on the person, for an answer or an approval. It needs a look as a reply nobody has
/// read does, so a name stands out for either.
pub(crate) fn waiting_on_you<'a>(chats: impl IntoIterator<Item = &'a Session>) -> bool {
    chats.into_iter().any(|chat| chat.is_open() && chat.status == SessionStatus::WaitingApproval)
}

#[cfg(test)]
mod tests {
    use look::Signal;
    use protocol::event::{AgentTask, TaskKind, TurnOutcome};
    use protocol::model::{Session, SessionStatus};

    use super::{chat, waiting_on_you, workspace};

    fn session(status: SessionStatus) -> Session {
        Session { status, ..protocol::fixtures::session("s") }
    }

    fn closed(status: SessionStatus) -> Session {
        Session { closed_at_ms: Some(1), ..session(status) }
    }

    /// A chat whose turn ended with these still running.
    fn left_running(kinds: &[TaskKind]) -> Session {
        let task = |kind: &TaskKind| AgentTask { id: "t".into(), description: None, kind: *kind, call: None };
        Session { background_tasks: kinds.iter().map(task).collect(), ..session(SessionStatus::Background) }
    }

    #[test]
    fn the_chat_with_the_most_going_on_speaks_for_the_workspace_wherever_it_is() {
        use SessionStatus::{Idle, Interrupted, Running, WaitingApproval};
        let chats = [session(WaitingApproval), session(Running), session(Idle)];
        assert_eq!(workspace(&chats), Signal::Working);
        let chats = [session(Idle), session(Running), session(Interrupted)];
        assert_eq!(workspace(&chats), Signal::Working);
        let chats = [session(Interrupted), left_running(&[TaskKind::Subagent]), session(Idle)];
        assert_eq!(workspace(&chats), Signal::Working);
    }

    #[test]
    fn a_turn_that_failed_stops_the_chat_short_over_what_it_left_going_until_a_turn_starts() {
        let failed = |chat: Session| Session { reply_outcome: Some(TurnOutcome::Error), ..chat };

        assert_eq!(chat(&failed(session(SessionStatus::Idle))), Signal::Failed);
        assert_eq!(chat(&failed(left_running(&[TaskKind::Subagent]))), Signal::Failed);
        assert_eq!(chat(&failed(session(SessionStatus::Running))), Signal::Working, "a turn started since");
        let stopped = Session { reply_outcome: Some(TurnOutcome::Cancelled), ..session(SessionStatus::Idle) };
        assert_eq!(chat(&stopped), Signal::Idle, "the person stopped it");
        assert_eq!(
            workspace(&[left_running(&[TaskKind::Command]), failed(session(SessionStatus::Idle))]),
            Signal::Failed
        );
    }

    #[test]
    fn a_chat_that_stopped_short_shows_over_one_that_only_left_something_running() {
        let chats = [left_running(&[TaskKind::Command]), session(SessionStatus::Interrupted)];
        assert_eq!(workspace(&chats), Signal::Failed);
        let chats = [session(SessionStatus::Idle), left_running(&[TaskKind::Monitor])];
        assert_eq!(workspace(&chats), Signal::Background);
    }

    #[test]
    fn a_closed_chat_doesnt_count() {
        use SessionStatus::{Idle, Running};
        assert_eq!(workspace(&[closed(Running), session(Idle)]), Signal::Idle);
        assert_eq!(workspace(&[closed(Running)]), Signal::Idle);
        assert_eq!(workspace(&[]), Signal::Idle);
    }

    #[test]
    fn the_light_sweeps_only_for_the_agents_own_work() {
        assert_eq!(chat(&session(SessionStatus::Running)), Signal::Working);
        assert_eq!(chat(&left_running(&[TaskKind::Command, TaskKind::Subagent])), Signal::Working);
        assert_eq!(chat(&left_running(&[TaskKind::Command])), Signal::Background);
        assert_eq!(chat(&left_running(&[TaskKind::Monitor])), Signal::Background);
        // To the person, a wait on the pull request is work in the background like any other. Only the hub keeps it.
        assert_eq!(chat(&left_running(&[TaskKind::Wait])), Signal::Background);
    }

    #[test]
    fn a_chat_waiting_on_the_person_holds_its_mark_still_and_says_so_apart_from_it() {
        use SessionStatus::{Idle, Running, WaitingApproval};
        assert_eq!(chat(&session(WaitingApproval)), Signal::Idle);

        assert!(waiting_on_you(&[session(Running), session(WaitingApproval)]));
        assert!(!waiting_on_you(&[session(Running), session(Idle)]));
        assert!(!waiting_on_you(&[closed(WaitingApproval)]), "a closed chat doesn't count");
    }

    /// The hub says a reply is unread on the same record, and the mark says the same either way.
    #[test]
    fn a_reply_nobody_has_read_leaves_the_mark_as_it_is() {
        let unread = |chat: Session| Session { reply_seq: 2, read_seq: 1, ..chat };
        for chat_of in [|| session(SessionStatus::Idle), || left_running(&[TaskKind::Command])] {
            assert!(unread(chat_of()).unread());
            assert_eq!(chat(&unread(chat_of())), chat(&chat_of()));
        }
    }
}
