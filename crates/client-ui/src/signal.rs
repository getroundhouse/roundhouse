//! What a chat's mark says of its agent, which chat's a workspace's mark shows, and the colour it's in for how the
//! workspace's pull request is getting on. Whether a chat is waiting on the person its name says rather than its
//! mark, as `client_core::marking` works out. Every app shows them the same way.

use client_core::marking::Mark;
use gpui_kit::assets::IconName;
use look::{Sign, Signal, Work};
use protocol::event::TaskKind;
use protocol::model::{
    OwnPullRequest, Phase, PullRequestFound, Session, SessionStatus, SetupStatus, Standing, Workspace,
};

/// A workspace's mark, of its record and its chats: being set up, until it is, and then the open chat with the most
/// going on speaks for it. A reply nobody has read isn't on the mark,
/// and neither is a chat waiting on an answer: the name beside it says to look, so that shows along with whatever
/// the agent is doing.
pub fn workspace_signal(workspace: &Workspace, chats: &[Session]) -> Signal {
    if workspace.setup == SetupStatus::Running {
        return Signal::SettingUp;
    }
    let urgency = |signal: &Signal| match signal {
        Signal::Working | Signal::SettingUp => 3,
        Signal::Failed => 2,
        Signal::Background => 1,
        Signal::Idle => 0,
    };
    let open = chats.iter().filter(|chat| chat.is_open());
    open.map(chat_signal).max_by_key(urgency).unwrap_or(Signal::Idle)
}

/// A chat's mark. The light sweeps only for the agent's own work: a turn, or a subagent it left going
/// when its turn ended. Anything else left going might end by itself, as a test run does, or be held open, as an app
/// is, and the agent doesn't say which. So that gets a quieter light, which claims only that something is running.
/// An agent that stopped short, by going away mid-turn or failing its turn, needs the person more than anything it
/// left going does.
pub fn chat_signal(session: &Session) -> Signal {
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

/// The colour of a workspace's work, as what the hub `found` of its pull request says, for its mark. `None` until the
/// hub has looked.
pub fn recorded_work(found: Option<&PullRequestFound>) -> Option<Work> {
    found.map(|found| work(found.pull_request.as_ref()))
}

/// The colour `pull_request` is shown in, on a workspace's mark and the desktop's pull request bar: what it is, as
/// GitHub colours it, whatever's in the way of merging it, which the bar's [`sign`] and words say. So a draft with
/// conflicts is a draft's grey, and an open one with failed checks is green.
pub fn work(pull_request: Option<&OwnPullRequest>) -> Work {
    let Some(own) = pull_request else { return Work::NoPr };
    match own.pull_request.phase() {
        Phase::Merged => Work::Merged,
        Phase::Closed => Work::Closed,
        Phase::Queued(_) => Work::Queued,
        Phase::Draft => Work::Draft,
        // A state this version doesn't know is still a pull request on GitHub, as far as anyone here knows.
        Phase::Open | Phase::Unknown => Work::Open,
    }
}

/// What kind of thing `standing` is in the way of merging, which the bar shows before its words. None for work in the
/// worktree, whose button says what to do, and for what's done, queued or can't be told, whose words say it all.
pub fn sign(standing: Standing) -> Option<Sign> {
    match standing {
        Standing::Conflicts | Standing::ChangesRequested | Standing::ChecksFailed(_) => Some(Sign::Problem),
        Standing::ChecksRunning(_)
        | Standing::Reading
        | Standing::ReviewRequired
        | Standing::Blocked
        | Standing::Checking => Some(Sign::Waiting),
        Standing::Ready => Some(Sign::Ready),
        Standing::NoPullRequest
        | Standing::Merged
        | Standing::Closed
        | Standing::Queued(_)
        | Standing::Uncommitted(_)
        | Standing::Diverged
        | Standing::Behind(_)
        | Standing::Unpushed(_)
        | Standing::Draft
        | Standing::Unknown => None,
    }
}

/// The row of a chat's or a workspace's menu that marks it as `mark` says, in words, with an open envelope to mark it
/// read and a closed one to mark it unread again, as Mail's are. What choosing it does is the menu's.
pub fn mark_row(mark: Mark) -> look::MenuRow {
    let icon = match mark {
        Mark::Read => IconName::MailOpen,
        Mark::Unread => IconName::Mail,
    };
    look::menu_row(format!("Mark as {}", mark.word())).icon(icon)
}

#[cfg(test)]
mod tests {
    use look::{Sign, Signal, Work};
    use protocol::event::{AgentTask, TaskKind, TurnOutcome};
    use protocol::model::{
        CheckState, Local, MergeState, OwnPullRequest, PrState, PullRequest, QueueState, Seq, Session, SessionStatus,
        SetupStatus, Standing, Workspace,
    };

    use super::{chat_signal as chat, sign, work};

    fn workspace(chats: &[Session]) -> Signal {
        super::workspace_signal(&protocol::fixtures::workspace("w"), chats)
    }

    fn session(status: SessionStatus) -> Session {
        Session { status, ..protocol::fixtures::session("s") }
    }

    fn closed(status: SessionStatus) -> Session {
        Session { closed_at_ms: Some(1), ..session(status) }
    }

    /// A chat whose turn ended with these still running.
    fn left_running(kinds: &[TaskKind]) -> Session {
        let task = |kind: &TaskKind| AgentTask::new("t", *kind);
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
        let setting_up = Workspace { setup: SetupStatus::Running, ..protocol::fixtures::workspace("w") };
        assert_eq!(super::workspace_signal(&setting_up, &chats), Signal::SettingUp, "until it's set up");
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
    fn a_chat_waiting_on_the_person_holds_its_mark_still_since_its_name_says_so() {
        assert_eq!(chat(&session(SessionStatus::WaitingApproval)), Signal::Idle);
    }

    /// The hub says a reply is unread on the same record, and the mark says the same either way.
    #[test]
    fn a_reply_nobody_has_read_leaves_the_mark_as_it_is() {
        let unread = |chat: Session| Session { reply_seq: Seq::new(2), read_seq: Seq::new(1), ..chat };
        for chat_of in [|| session(SessionStatus::Idle), || left_running(&[TaskKind::Command])] {
            assert!(unread(chat_of()).unread());
            assert_eq!(chat(&unread(chat_of())), chat(&chat_of()));
        }
    }

    #[test]
    fn a_pull_request_is_coloured_by_what_it_is_whatever_is_in_its_way() {
        let open = protocol::fixtures::pull_request(7);
        let with = |change: fn(&mut PullRequest)| {
            let mut pull_request = open.clone();
            change(&mut pull_request);
            OwnPullRequest { pull_request, local: Local::default() }
        };
        let passed = |pr: &mut PullRequest| pr.checks.as_mut().unwrap().said.checks[0].state = CheckState::Passed;
        let cases = [
            (with(|_| {}), Work::Open),
            (with(passed), Work::Open),
            (with(|pr| pr.checks = None), Work::Open),
            (
                with(|pr| (pr.summary.said.is_draft, pr.summary.said.merge_state) = (true, MergeState::Dirty)),
                Work::Draft,
            ),
            (with(|pr| pr.summary.said.queue = Some(QueueState::Mergeable)), Work::Queued),
            (with(|pr| pr.summary.said.state = PrState::Merged), Work::Merged),
            (with(|pr| pr.summary.said.state = PrState::Closed), Work::Closed),
            (with(|pr| pr.summary.said.state = PrState::Unknown), Work::Open),
        ];

        let worked: Vec<Work> = cases.iter().map(|(pr, _)| work(Some(pr))).collect();

        assert_eq!(worked, cases.map(|(_, work)| work));
        assert_eq!(work(None), Work::NoPr);
    }

    /// What has to be fixed is a problem, what waits on GitHub or someone else is waiting, and what the worktree has
    /// to do, or what's done or queued, has no sign, since its button or its words say it.
    #[test]
    fn whats_in_the_way_is_a_problem_something_waited_on_or_nothing() {
        let counts = protocol::model::CheckCounts::default();
        let cases = [
            (Standing::Conflicts, Some(Sign::Problem)),
            (Standing::ChangesRequested, Some(Sign::Problem)),
            (Standing::ChecksFailed(counts), Some(Sign::Problem)),
            (Standing::ChecksRunning(counts), Some(Sign::Waiting)),
            (Standing::Reading, Some(Sign::Waiting)),
            (Standing::ReviewRequired, Some(Sign::Waiting)),
            (Standing::Ready, Some(Sign::Ready)),
            (Standing::Uncommitted(1), None),
            (Standing::Draft, None),
            (Standing::Queued(QueueState::Mergeable), None),
            (Standing::Merged, None),
        ];

        let signed = cases.map(|(standing, _)| sign(standing));

        assert_eq!(signed, cases.map(|(_, sign)| sign));
    }
}
