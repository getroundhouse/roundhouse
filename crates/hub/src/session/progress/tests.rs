//! What one step does to everything an event changes together, which the rules in `status` and the queue's hold each
//! decide a part of.

use std::collections::HashMap;

use protocol::event::{AgentTask, ApprovalOutcome, ExitReason, NoticeLevel, TaskKind, TurnOutcome};
use protocol::model::ApprovalId;
use protocol::prompt::Prompt;
use serde_json::json;

use super::*;
use crate::store::TurnEdge;

fn progress(status: SessionStatus, tasks: usize) -> Progress {
    let task =
        |i: usize| AgentTask { id: format!("t{i}").into(), description: None, kind: TaskKind::Command, call: None };
    Progress {
        status,
        approvals: HashMap::new(),
        background_tasks: (0..tasks).map(task).collect(),
        turn_started_at_ms: Some(1_000),
        queue: QueueWatch::default(),
        outlives_this_hub: false,
    }
}

fn current(run: u64) -> Origin {
    Origin::Run { run: RunId::new(run), current: true, numbered: None }
}

fn asked(id: &str) -> EventBody {
    serde_json::from_value(json!({"type": "approval_requested", "approval_id": id, "tool_name": "Bash", "input": {}}))
        .unwrap()
}

fn ended(started_at_ms: Option<i64>) -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
    }
}

fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: Vec::new() }
}

#[test]
fn a_turn_starts_when_its_event_is_recorded_and_its_end_says_so_whatever_the_agent_said() {
    let mut progress = progress(SessionStatus::Idle, 0);
    let prompt = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("x") };

    let Step { change, next, .. } = progress.step(Origin::Hub, prompt, true);
    progress.take(next, 42, Some(vec![1]));

    assert_eq!(
        change.status.map(|status| (status.status, status.turn)),
        Some((SessionStatus::Running, TurnEdge::Starts))
    );
    assert_eq!(progress.turn_started_at_ms, Some(42));
    let Step { body, change, .. } = progress.step(current(1), ended(Some(7)), true);
    assert!(matches!(body, EventBody::TurnEnded { started_at_ms: Some(42), .. }), "{body:?}");
    assert_eq!(change.status.map(|status| (status.status, status.turn)), Some((SessionStatus::Idle, TurnEdge::Ends)));
}

#[test]
fn the_agent_going_away_mid_turn_ends_its_work_and_holds_the_queue_in_one_change() {
    let progress = progress(SessionStatus::Background, 2);

    let Step { change, .. } = progress.step(current(1), exited(), true);

    let status = change.status.unwrap();
    assert_eq!(
        (status.status, status.turn, status.background_tasks),
        (SessionStatus::Interrupted, TurnEdge::Neither, Some(Vec::new()))
    );
    assert_eq!(change.queue_held, Some(true));
}

/// As the hub stops, a chat whose only work is a wait the next hub keeps is left as that hub finds it, and one at work
/// on anything else is cut off.
#[test]
fn an_agent_that_goes_with_the_hub_cuts_off_nothing_but_its_work_besides_a_wait() {
    let wait = AgentTask { id: "w".into(), description: None, kind: TaskKind::Wait, call: None };
    let gone = |outlives: bool, tasks: Vec<AgentTask>| {
        let mut progress = progress(SessionStatus::Background, 0);
        progress.background_tasks = tasks;
        progress.queue = QueueWatch::starting(true);
        progress.outlive_this_hub(outlives);
        let Step { change, next, .. } = progress.step(current(1), exited(), true);
        progress.take(next, 5, None);
        (change.status.map(|status| status.status), change.queue_held, progress.background_tasks.len())
    };

    assert_eq!(gone(true, vec![wait.clone()]), (None, Some(false), 1), "the wait is still listed, and the queue free");
    let cut_off = (Some(SessionStatus::Interrupted), None, 0);
    assert_eq!(gone(false, vec![wait.clone()]), cut_off, "no wait is open");
    assert_eq!(gone(true, progress(SessionStatus::Background, 1).background_tasks), cut_off, "it's not only a wait");
}

#[test]
fn what_a_replaced_agent_asks_stays_open_but_changes_nothing_on_the_record() {
    let mut progress = progress(SessionStatus::Running, 1);
    let replaced = Origin::Run { run: RunId::new(1), current: false, numbered: None };

    let Step { change, next, .. } = progress.step(replaced, asked("a"), true);
    progress.take(next, 5, None);

    assert_eq!(change, SessionChange::default());
    assert_eq!(progress.status, SessionStatus::Running);
    assert_eq!(progress.approvals.get(&ApprovalId::from("a")), Some(&Some(RunId::new(1))), "until it's cancelled");
    let Step { change, .. } = progress.step(replaced, exited(), true);
    assert_eq!(change, SessionChange::default(), "its exit isn't the current agent's");
}

#[test]
fn an_approval_is_open_from_its_asking_until_its_answer() {
    let mut progress = progress(SessionStatus::Running, 0);

    let Step { change, next, .. } = progress.step(current(2), asked("a"), true);
    progress.take(next, 5, None);
    assert_eq!(change.status.map(|status| status.status), Some(SessionStatus::WaitingApproval));
    let answered = EventBody::ApprovalResolved {
        approval_id: "a".into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
    };
    let Step { change, next, .. } = progress.step(Origin::Hub, answered, true);
    progress.take(next, 6, None);

    assert_eq!(change.status.map(|status| status.status), Some(SessionStatus::Running));
    assert!(progress.approvals.is_empty());
}

#[test]
fn an_event_that_changes_nothing_has_nothing_to_commit() {
    let progress = progress(SessionStatus::Running, 1);
    let notice = EventBody::Notice { level: NoticeLevel::Info, message: "x".into() };

    let Step { change, .. } = progress.step(current(1), notice, true);

    assert!(change.is_none());
}
