use serde_json::{Value, json};

use super::*;
use crate::capability::output::{Thinking, ThinkingForm};
use crate::capability::{Capabilities, Capability, approvals, asking, background, context, turns};
use crate::capability::{Claimed, approvals as approval_claims, background as background_claims};
use crate::event::{AgentTask, ApprovalOutcome, ExitReason, SubagentOutcome, TokenUsage, TurnOutcome};
use crate::event::{Asking, CompactedBy, CompactionOutcome, ContentBlock, EventBody, SessionEvent, TaskKind};
use crate::fixtures::{event, tool_result, tool_use, turn_ended};
use crate::model::PermissionMode;
use crate::model::TaskId;
use crate::model::{RunId, Seq};
use crate::prompt::Prompt;

/// `body` at `seq`, from the agent's launch `run`, or about it, as its start and its exit are.
fn from_run(run: u64, seq: u64, body: EventBody) -> SessionEvent {
    SessionEvent { run: Some(RunId::new(run)), ..event(Seq::new(seq), body) }
}

/// `body` at `seq`, recorded by the hub, which gives it no run.
fn from_hub(seq: u64, body: EventBody) -> SessionEvent {
    event(Seq::new(seq), body)
}

fn prompt() -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("fix the build") }
}

fn taken(prompt: u64) -> EventBody {
    EventBody::PromptTaken { prompt: Seq::new(prompt) }
}

fn dropped(prompt: u64) -> EventBody {
    EventBody::PromptDropped { prompt: Seq::new(prompt) }
}

fn said(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

fn sent_back(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::AgentUser { parent_tool_use_id: None, blocks }
}

fn asked(id: &str) -> EventBody {
    EventBody::ApprovalRequested {
        request: None,
        approval_id: id.into(),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: json!({"command": "ls"}),
        description: None,
        suggestions: Box::new(Value::Null),
        asking: None,
        reason: None,
        asked_by: None,
    }
}

fn resolved(id: &str) -> EventBody {
    EventBody::ApprovalResolved {
        approval_id: id.into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
        by_setting: false,
    }
}

fn subagent_started(call: &str) -> EventBody {
    EventBody::SubagentStarted { call: call.into(), kind: None, description: None, prompt: None, in_background: false }
}

fn subagent_ended(call: &str) -> EventBody {
    EventBody::SubagentEnded {
        call: call.into(),
        outcome: SubagentOutcome::Finished,
        report: None,
        context_tokens: None,
    }
}

fn started() -> EventBody {
    EventBody::AgentStarted { pid: 1, resumed: false, build: None, previous_build: None, capabilities: None }
}

fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(1), signal: None, stderr_tail: vec![] }
}

/// A run that asks before tools and hands work to subagents, and claims nothing else.
fn claims() -> Capabilities {
    Capabilities {
        approvals: approval_claims::Claims {
            requests: Some(approvals::Requests { parts: vec![], kept: vec![] }),
            ..Default::default()
        },
        background: background_claims::Claims { subagents: Some(Claimed {}), ..Default::default() },
        ..Capabilities::default()
    }
}

/// What `log` breaks, where every run claims `claims`.
fn breaches(log: &[SessionEvent], claims: &Capabilities) -> Vec<(Law, u64)> {
    by_run(log, |_| claims)
}

fn by_run<'c>(log: &[SessionEvent], claims: impl Fn(RunId) -> &'c Capabilities) -> Vec<(Law, u64)> {
    check(log, claims).into_iter().map(|breach| (breach.law, breach.seq.get())).collect()
}

#[test]
fn a_session_that_keeps_every_law_breaks_none() {
    let log = [
        from_run(1, 1, started()),
        from_hub(2, prompt()),
        from_run(1, 3, taken(2)),
        from_run(1, 4, said(vec![tool_use("t", "Bash", json!({"command": "ls"}))])),
        from_run(1, 5, asked("a")),
        from_hub(6, resolved("a")),
        from_run(1, 7, resolved("a")),
        from_run(1, 8, sent_back(vec![tool_result("t", "Cargo.toml", false)])),
        from_run(1, 9, subagent_started("t")),
        from_run(1, 10, turn_ended(TurnOutcome::Success)),
        from_hub(11, prompt()),
        from_run(1, 12, exited()),
        from_hub(13, dropped(11)),
        from_hub(14, subagent_ended("t")),
        from_run(2, 15, started()),
        from_run(2, 16, taken(11)),
        from_run(2, 17, subagent_ended("t")),
    ];

    assert_eq!(breaches(&log, &claims()), []);
}

#[test]
fn a_prompt_is_settled_after_it_was_recorded_by_the_hub_before_the_agent_and_by_each_once() {
    let asked_to_compact = EventBody::CompactionAsked { keeping: None };
    let log = [
        from_hub(1, prompt()),
        from_hub(2, prompt()),
        from_hub(3, dropped(1)),
        from_run(1, 4, taken(1)),
        from_run(1, 5, taken(1)),
        from_run(1, 6, taken(2)),
        from_hub(7, dropped(2)),
        from_run(1, 8, taken(9)),
        from_hub(9, asked_to_compact),
        from_run(1, 10, taken(9)),
    ];

    let expected = [5, 7, 8].map(|seq| (Law::PromptSettlesOnce, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

#[test]
fn a_request_is_resolved_after_it_was_made_by_the_hub_before_the_agent_and_by_each_once() {
    let log = [
        from_hub(1, resolved("a")),
        from_run(1, 2, asked("a")),
        from_hub(3, resolved("a")),
        from_run(1, 4, resolved("a")),
        from_run(1, 5, resolved("a")),
        from_run(1, 6, asked("b")),
        from_run(1, 7, resolved("b")),
        from_hub(8, resolved("b")),
    ];

    let expected = [1, 5, 8].map(|seq| (Law::ApprovalResolvesOnce, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

#[test]
fn a_subagent_ends_after_it_started_by_the_hub_before_the_agent_and_by_each_once() {
    let log = [
        from_run(1, 1, subagent_ended("t")),
        from_run(1, 2, subagent_started("t")),
        from_hub(3, subagent_ended("t")),
        from_run(1, 4, subagent_ended("t")),
        from_run(1, 5, subagent_ended("t")),
        from_run(1, 6, subagent_started("u")),
        from_hub(7, subagent_ended("u")),
        from_hub(8, subagent_ended("u")),
        from_run(1, 9, subagent_started("v")),
        from_run(1, 10, subagent_ended("v")),
        from_hub(11, subagent_ended("v")),
    ];

    let expected = [1, 5, 8, 11].map(|seq| (Law::SubagentEndsOnce, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

#[test]
fn a_call_has_one_result_and_only_after_it() {
    let log = [
        from_run(1, 1, sent_back(vec![tool_result("early", "?", false)])),
        from_run(1, 2, said(vec![tool_use("t", "Read", json!({"file_path": "a"}))])),
        from_run(1, 3, sent_back(vec![tool_result("t", "1\tfn main() {}", false)])),
        from_run(1, 4, sent_back(vec![tool_result("t", "again", false)])),
    ];

    assert_eq!(breaches(&log, &claims()), [(Law::ResultFollowsCall, 1), (Law::ResultFollowsCall, 4)]);
}

#[test]
fn nothing_of_a_run_comes_after_it_exits_but_its_start_and_the_hub_still_records() {
    let log = [
        from_run(1, 1, asked("a")),
        from_run(1, 2, exited()),
        from_hub(3, resolved("a")),
        from_run(1, 4, turn_ended(TurnOutcome::Success)),
        from_run(1, 5, said(vec![])),
        from_run(1, 6, asked("b")),
        from_run(1, 7, started()),
        from_run(1, 8, exited()),
        from_run(2, 9, turn_ended(TurnOutcome::Success)),
    ];

    let expected = [4, 5, 6, 8].map(|seq| (Law::NothingAfterExit, seq));
    assert_eq!(breaches(&log, &claims()), expected);
}

#[test]
fn what_a_run_records_needs_only_what_it_claims_and_the_hubs_own_records_are_its_own() {
    let thought = || said(vec![ContentBlock::Thinking { text: "hm".into() }]);
    let thinking = Thinking { default: ThinkingForm::Summary, others: vec![] };
    let thinks = Capabilities {
        output: crate::capability::output::Claims { thinking: Some(thinking), ..Default::default() },
        ..claims()
    };
    let log = [from_run(1, 1, thought()), from_hub(2, thought()), from_run(1, 3, said(vec![]))];

    assert_eq!(breaches(&log, &claims()), [(Law::OnlyWhatsClaimed, 1)]);
    assert_eq!(breaches(&log, &thinks), []);
    assert_eq!(breaches(&[from_run(1, 1, asked("a"))], &Capabilities::default()), [(Law::OnlyWhatsClaimed, 1)]);
    let (plain, two_runs) = (claims(), [from_run(1, 1, thought()), from_run(2, 2, thought())]);
    let first_thinks = |run: RunId| if run == RunId::new(1) { &thinks } else { &plain };
    assert_eq!(by_run(&two_runs, first_thinks), [(Law::OnlyWhatsClaimed, 2)]);
}

fn task(kind: TaskKind) -> AgentTask {
    AgentTask { id: TaskId::from("t"), description: None, kind, call: None }
}

#[test]
fn each_task_needs_the_capability_its_kind_of_task_is() {
    let tasks = |kinds: &[TaskKind]| EventBody::AgentTasks {
        tasks: kinds.iter().copied().map(task).collect(),
        stopped: vec![],
    };

    assert_eq!(needs(&tasks(&[])), []);
    assert_eq!(
        needs(&tasks(&[TaskKind::Subagent, TaskKind::Monitor, TaskKind::Command, TaskKind::Wait])),
        [
            Capability::from(background::Capability::Subagents),
            background::Capability::BackgroundCommands.into(),
            background::Capability::BackgroundCommands.into(),
            background::Capability::ToolTasks.into(),
        ]
    );
}

#[test]
fn a_turns_end_needs_what_it_says_beyond_how_it_ended() {
    let tokens = TokenUsage { input_tokens: 1, output_tokens: 1, cache_read_tokens: 0, cache_write_tokens: 0 };
    let plain = turn_ended(TurnOutcome::Success);
    let mut full = plain.clone();
    if let EventBody::TurnEnded { cost_usd, tokens: told, only_compacted, .. } = &mut full {
        *cost_usd = Some(0.5);
        *told = Some(tokens);
        *only_compacted = true;
    }

    assert_eq!(needs(&plain), []);
    assert_eq!(
        needs(&full),
        [
            Capability::from(turns::Capability::Tokens),
            turns::Capability::Cost.into(),
            context::Capability::Compaction.into(),
        ]
    );
}

#[test]
fn a_compaction_needs_what_it_reports() {
    let compacted = |by, before_tokens, summary: Option<&str>| EventBody::AgentCompacted {
        asked: None,
        keeping: None,
        outcome: CompactionOutcome::Compacted {
            by,
            before_tokens,
            after_tokens: None,
            took_ms: None,
            summary: summary.map(String::from),
        },
    };

    assert_eq!(needs(&compacted(CompactedBy::Asked, None, None)), []);
    let mut answers = compacted(CompactedBy::Asked, None, None);
    if let EventBody::AgentCompacted { asked, .. } = &mut answers {
        *asked = Some(Seq::new(1));
    }
    assert_eq!(needs(&answers), [context::Capability::Compaction.into()]);
    assert_eq!(needs(&compacted(CompactedBy::Agent, None, None)), [context::Capability::AutoCompaction.into()]);
    assert_eq!(needs(&compacted(CompactedBy::Asked, Some(9), None)), [context::Capability::CompactionTokens.into()]);
    assert_eq!(
        needs(&compacted(CompactedBy::Asked, None, Some("so far"))),
        [context::Capability::CompactionSummary.into()]
    );
    assert_eq!(
        needs(&EventBody::AgentCompacting { written_chars: Some(10) }),
        [context::Capability::CompactionProgress.into()]
    );
}

#[test]
fn what_the_agent_says_of_its_mode_limits_and_questions_needs_their_capabilities() {
    let asking = |asking| {
        let EventBody::ApprovalRequested {
            request: None,
            approval_id,
            tool_name,
            tool_use_id,
            input,
            description,
            suggestions,
            reason,
            asked_by,
            ..
        } = asked("a")
        else {
            unreachable!()
        };
        EventBody::ApprovalRequested {
            request: None,
            approval_id,
            tool_name,
            tool_use_id,
            input,
            description,
            suggestions,
            asking: Some(Box::new(asking)),
            reason,
            asked_by,
        }
    };

    assert_eq!(
        needs(&EventBody::AgentMode { permission_mode: PermissionMode::Plan }),
        [approvals::Capability::Modes.into()]
    );
    assert_eq!(
        needs(&EventBody::AgentLimits { limits: vec![] }),
        [crate::capability::account::Capability::Limits.into()]
    );
    assert_eq!(needs(&asking(Asking::Plan { plan: "1.".into() })), [asking::Capability::PlanApproval.into()]);
    assert_eq!(needs(&asking(Asking::Questions { questions: vec![] })), [asking::Capability::Questions.into()]);
    assert_eq!(needs(&asked("a")), [approvals::Capability::Requests.into()]);
    let tool_use = Asking::ToolUse { call: Some(crate::event::CallKind::Command) };
    assert_eq!(needs(&asking(tool_use)), [approvals::Capability::Requests.into()]);
    assert_eq!(needs(&asking(Asking::Unknown)), [approvals::Capability::Requests.into()]);
}

#[test]
fn what_the_agent_says_of_its_subagents_needs_them() {
    let subagents = [Capability::from(background::Capability::Subagents)];

    assert_eq!(needs(&subagent_started("t")), subagents);
    assert_eq!(needs(&EventBody::SubagentContext { call: "t".into(), tokens: 9 }), subagents);
    assert_eq!(needs(&subagent_ended("t")), subagents);
}

#[test]
fn typed_requests_need_their_own_capabilities() {
    let mut body = asked("q");
    let EventBody::ApprovalRequested { request, .. } = &mut body else { unreachable!() };
    *request = Some(Box::new(approvals::Request {
        asks: approvals::Asks::Questions { questions: Vec::new() },
        suggested: Vec::new(),
        always: false,
        why: None,
    }));
    let log = [from_run(1, 1, body)];
    let mut claims = Capabilities::default();
    claims.approvals.requests = Some(approvals::Requests { parts: Vec::new(), kept: Vec::new() });
    assert_eq!(check(&log, |_| &claims), [Breach { law: Law::OnlyWhatsClaimed, seq: Seq::FIRST }]);
    claims.asking.questions = Some(Claimed {});
    assert!(check(&log, |_| &claims).is_empty());
}
