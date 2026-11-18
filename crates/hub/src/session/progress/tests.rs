//! What one step does to everything an event changes together, which the rules in `status` and the queue's hold each
//! decide a part of.

use std::collections::HashMap;

use protocol::capability::turns::OwnTurn;
use protocol::event::{
    Again, AgentRunState, AgentTask, ContentBlock, ExitReason, FailureKind, NoticeLevel, TaskChange, TaskKind,
    TaskOutcome, TurnOutcome,
};
use protocol::model::ApprovalId;
use protocol::prompt::Prompt;
use serde_json::json;

use super::*;
use crate::store::TurnEdge;

fn progress(status: SessionStatus, tasks: usize) -> Progress {
    let task = |i: usize| AgentTask {
        id: format!("t{i}").into(),
        description: None,
        kind: TaskKind::Command,
        call: None,
        subagent: None,
    };
    Progress {
        status,
        approvals: HashMap::new(),
        background_tasks: Vec::from_fn(tasks, task),
        turn_started_at_ms: Some(1_000),
        queue: QueueWatch::default(),
        outlives_this_hub: false,
        opened_by: None,
        failing: None,
    }
}

fn current(run: u64) -> Origin {
    Origin::Run { run: RunId::new(run), current: true, numbered: None }
}

fn asked(id: &str) -> EventBody {
    protocol::fixtures::approval_request(id, protocol::fixtures::to_run("ls")).into()
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
        request_failed: false,
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
    progress.take(next, Seq::new(1), 42, Some(vec![Seq::new(1)]));

    assert_eq!(
        change.status.map(|status| (status.status, status.turn)),
        Some((SessionStatus::Running, TurnEdge::Starts))
    );
    assert_eq!(progress.turn_started_at_ms, Some(42));
    let Step { body, change, .. } = progress.step(current(1), ended(Some(7)), true);
    assert!(matches!(body, EventBody::TurnEnded { started_at_ms: Some(42), .. }), "{body:?}");
    assert_eq!(
        change.status.map(|status| (status.status, status.turn)),
        Some((SessionStatus::Idle, TurnEdge::Ends(Some(TurnOutcome::Success))))
    );
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
    let wait = AgentTask { id: "w".into(), description: None, kind: TaskKind::Wait, call: None, subagent: None };
    let gone = |outlives: bool, tasks: Vec<AgentTask>| {
        let mut progress = progress(SessionStatus::Background, 0);
        progress.background_tasks = tasks;
        progress.queue = QueueWatch::starting(true);
        progress.outlive_this_hub(outlives);
        let Step { change, next, .. } = progress.step(current(1), exited(), true);
        progress.take(next, Seq::new(5), 5, None);
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
    progress.take(next, Seq::new(5), 5, None);

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
    progress.take(next, Seq::new(5), 5, None);
    assert_eq!(change.status.map(|status| status.status), Some(SessionStatus::WaitingApproval));
    let answered = protocol::fixtures::decided("a", protocol::fixtures::allow_once());
    let Step { change, next, .. } = progress.step(Origin::Hub, answered, true);
    progress.take(next, Seq::new(6), 6, None);

    assert_eq!(change.status.map(|status| status.status), Some(SessionStatus::Running));
    assert!(progress.approvals.is_empty());
}

#[test]
fn an_event_that_changes_nothing_has_nothing_to_commit() {
    let progress = progress(SessionStatus::Running, 1);
    let notice = EventBody::Notice { level: NoticeLevel::Info, message: "x".into(), subagent: None };

    let Step { change, .. } = progress.step(current(1), notice, true);

    assert!(change.is_none());
}

/// The hub dropped the prompt that started the turn before the agent said anything of it, as when its files can't be
/// put in the worktree or its run refuses it, so the agent, running and idle, never says the turn ended.
#[test]
fn a_turn_whose_prompt_the_hub_dropped_before_the_agent_said_anything_never_started() {
    let prompt = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("x") };
    let dropped = || EventBody::PromptDropped { prompt: Seq::new(1) };
    // The status, the last event's edge, and whether the queue's held, once `opening` is recorded from `from`, and
    // then each of `then`, with an agent running if `agent`.
    let after = |from, opening: &EventBody, then: Vec<(Origin, EventBody)>, agent| {
        let mut progress = progress(from, 0);
        let mut turn = None;
        for (seq, (origin, body)) in (1..).zip(std::iter::once((Origin::Hub, opening.clone())).chain(then)) {
            let settles = matches!(
                body,
                EventBody::UserPrompt { .. } | EventBody::CompactionAsked { .. } | EventBody::PromptDropped { .. }
            );
            let untaken = settles.then(|| if seq == 1 { vec![Seq::new(1)] } else { Vec::new() });
            let Step { change, next, .. } = progress.step(origin, body, agent);
            progress.take(next, Seq::new(seq), 5, untaken);
            turn = change.status.map(|status| status.turn);
        }
        (progress.status, turn, progress.queue.is_held())
    };
    let idle = SessionStatus::Idle;
    let never_started = (idle, Some(TurnEdge::Neither), true);
    let left = (SessionStatus::Running, None, false);
    let alone = || vec![(Origin::Hub, dropped())];
    let said = |by, body| vec![(by, body), (Origin::Hub, dropped())];

    assert_eq!(after(idle, &prompt, alone(), true), never_started);
    assert_eq!(after(idle, &EventBody::CompactionAsked { keeping: None }, alone(), true), never_started);
    let started =
        EventBody::AgentStarted { pid: 1, resumed: false, build: None, previous_build: None, capabilities: None };
    assert_eq!(after(idle, &prompt, said(current(1), started), true), never_started, "its start isn't of the turn");
    let stale = EventBody::AgentState { state: AgentRunState::Idle };
    assert_eq!(after(idle, &prompt, said(current(1), stale), true), never_started, "an idle from before it isn't");
    let warned = EventBody::Notice { level: NoticeLevel::Warning, message: "x".into(), subagent: None };
    assert_eq!(
        after(idle, &prompt, said(Origin::Hub, warned), true),
        never_started,
        "the hub's words aren't the agent's"
    );
    let working = EventBody::AgentState { state: AgentRunState::Running };
    assert_eq!(
        after(idle, &prompt, said(current(1), working), true),
        left,
        "once it says it's working, its end decides"
    );
    assert_eq!(after(idle, &prompt, alone(), false), left, "with no agent running, its exit cuts the turn off");
    assert_eq!(after(SessionStatus::Running, &prompt, alone(), true), left, "a prompt sent mid-turn didn't start it");
    let gone = after(idle, &prompt, vec![(current(1), dropped()), (current(1), exited())], true);
    assert_eq!(
        gone,
        (SessionStatus::Interrupted, Some(TurnEdge::Neither), true),
        "an agent's own drop leaves its exit to say"
    );
}

fn failed(message: &str, subagent: Option<&str>) -> EventBody {
    protocol::fixtures::request_failed(message, subagent)
}

fn ended_on_a_failure() -> EventBody {
    let mut ended = protocol::fixtures::turn_ended(TurnOutcome::Error);
    if let EventBody::TurnEnded { request_failed, .. } = &mut ended {
        *request_failed = true;
    }
    ended
}

fn says(text: &str, subagent: Option<&str>) -> EventBody {
    let blocks = vec![ContentBlock::Text { text: text.into(), images: Vec::new() }];
    let subagent = subagent.map(Into::into);
    EventBody::Assistant { message_id: None, subagent, model: None, blocks, first_index: None }
}

/// What Claude's CLI sends between two failed requests of the agent's own as one of its tasks ends and it picks up
/// by itself, as the hub's own logs have it: a subagent's last words, its task's change and its end, the shorter list, the turn's start and
/// the agent's own lines with no form, its state, why it picked up, and its limits.
fn picks_up_by_itself() -> Vec<EventBody> {
    let other = |kind: &str| EventBody::AgentOther { kind: kind.into(), subagent: None };
    vec![
        says("Here's the review.", Some("toolu_1")),
        EventBody::SubagentContext { subagent: "toolu_1".into(), tokens: 9 },
        EventBody::TaskChanged { task: "t1".into(), change: TaskChange::Updated },
        EventBody::SubagentEnded {
            subagent: "toolu_1".into(),
            outcome: TaskOutcome::Finished,
            report: None,
            context_tokens: None,
        },
        EventBody::AgentTasks { tasks: Vec::new(), stopped: Vec::new() },
        EventBody::AgentState { state: AgentRunState::Running },
        serde_json::from_value(json!({"type": "agent_init", "agent_session_id": null, "model": null, "cwd": null,
            "mode": null, "agent_version": null}))
        .unwrap(),
        other("system/status"),
        EventBody::OwnTurn(OwnTurn::TaskEnded { ended: Vec::new() }),
        EventBody::AgentLimits { limits: Vec::new() },
    ]
}

/// Records each of `bodies`, from the hub if it's a prompt and otherwise from the current agent, from seq 1, and
/// returns what each failed request, a subagent's too, says it repeats, by its seq.
fn agains(bodies: Vec<EventBody>) -> Vec<(u64, Option<Again>)> {
    let origin = |body: &EventBody| if matches!(body, EventBody::UserPrompt { .. }) { Origin::Hub } else { current(1) };
    agains_from(bodies.into_iter().map(|body| (origin(&body), body)).collect())
}

/// The same, each from its own origin.
fn agains_from(bodies: Vec<(Origin, EventBody)>) -> Vec<(u64, Option<Again>)> {
    let mut progress = progress(SessionStatus::Running, 0);
    let mut said = Vec::new();
    for ((origin, body), seq) in bodies.into_iter().zip(1u64..) {
        let Step { body, next, .. } = progress.step(origin, body, true);
        progress.take(next, Seq::new(seq), seq.cast_signed(), None);
        if let EventBody::RequestFailed { again, .. } = body {
            said.push((seq, again));
        }
    }
    said
}

fn repeats(previous: u64, times: u32) -> Again {
    Again { previous: Seq::new(previous), times }
}

/// Each turn the agent started by itself as one of its tasks ended failed the same way again, with what the CLI sends
/// between, and one in between came to nothing, so each failure says it repeats the one before, and how many times it
/// has failed in a row.
#[test]
fn the_same_failure_after_turns_that_came_to_nothing_else_repeats_the_one_before() {
    let mut bodies = vec![failed("limit", None), ended_on_a_failure()];
    let second = bodies.len() + picks_up_by_itself().len() + 1;
    bodies.extend(picks_up_by_itself());
    bodies.extend([failed("limit", None), ended_on_a_failure()]);
    bodies.extend(picks_up_by_itself());
    bodies.push(ended(None));
    bodies.extend(picks_up_by_itself());
    let third = bodies.len() + 1;
    bodies.push(failed("limit", None));

    let second = u64::try_from(second).unwrap();
    let third = u64::try_from(third).unwrap();
    assert_eq!(agains(bodies), [(1, None), (second, Some(repeats(1, 2))), (third, Some(repeats(second, 3)))]);
}

/// A prompt, Retry's included, anything the agent says, a turn someone stopped, and the agent going, each end the row,
/// so the failure after starts another.
#[test]
fn anything_the_person_or_the_agent_does_between_two_failures_ends_the_row() {
    let prompt = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("Continue") };
    let cancelled = protocol::fixtures::turn_ended(TurnOutcome::Cancelled);
    for between in [prompt, says("Picking up where I left off.", None), cancelled, exited()] {
        let kind = between.kind();
        let bodies = vec![failed("limit", None), between, failed("limit", None)];
        assert_eq!(agains(bodies), [(1, None), (3, None)], "{kind}");
    }
}

/// Only the same failure carries on the row: one that differs only in when its limit resets, or only in its kind,
/// starts another. A subagent's is in its own tab, so it's no part of the agent's row, doesn't end it, and repeats
/// nothing, whatever it came with.
#[test]
fn another_failure_starts_a_row_and_a_subagents_is_no_part_of_one() {
    let mut resets_later = failed("limit", None);
    if let EventBody::RequestFailed { failure, .. } = &mut resets_later {
        failure.resets_at_ms = Some(9_000);
    }
    let mut subagents = failed("limit", Some("toolu_1"));
    if let EventBody::RequestFailed { again, .. } = &mut subagents {
        *again = Some(repeats(1, 2));
    }
    let mut overloaded = failed("limit", None);
    if let EventBody::RequestFailed { failure, .. } = &mut overloaded {
        failure.kind = FailureKind::Overloaded;
    }
    let bodies = vec![resets_later, failed("limit", None), subagents, failed("limit", None), overloaded];

    assert_eq!(agains(bodies), [(1, None), (2, None), (3, None), (4, Some(repeats(2, 2))), (5, None)]);
}

/// The failures in a row are the log's, which a session started again reads whoever said each event, so what an agent
/// that was replaced says ends the row as the current agent's words would, and its failure counts as one of the row,
/// with what the hub says of it rather than what it came with, though it changes nothing else.
#[test]
fn what_a_replaced_agent_says_ends_the_row_too() {
    let replaced = Origin::Run { run: RunId::new(1), current: false, numbered: None };
    let mut its_failure = failed("limit", None);
    if let EventBody::RequestFailed { again, .. } = &mut its_failure {
        *again = Some(repeats(9, 9));
    }
    let bodies = vec![
        (current(2), failed("limit", None)),
        (replaced, says("Picking up where I left off.", None)),
        (current(2), failed("limit", None)),
        (replaced, its_failure),
        (current(2), failed("limit", None)),
    ];

    let expected = [(1, None), (3, None), (4, Some(repeats(3, 2))), (5, Some(repeats(4, 3)))];
    assert_eq!(agains_from(bodies), expected);
}
