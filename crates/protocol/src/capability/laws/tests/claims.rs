use super::super::*;
use crate::capability::Claimed;
use crate::capability::output::{Thinking, ThinkingForm};
use crate::capability::{Capabilities, Capability, approvals, asking, background, context, turns};
use crate::event::{AgentTask, TokenUsage, TurnOutcome};
use crate::event::{CompactedBy, CompactionOutcome, ContentBlock, EventBody, TaskKind};
use crate::fixtures::{approval_request, subagent_started, turn_ended};
use crate::model::{PermissionMode, RunId, Seq, TaskId};

use super::log::*;

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
    AgentTask { id: TaskId::from("t"), description: None, kind, call: None, subagent: None }
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
fn a_turn_needs_what_its_start_and_end_say_beyond_how_it_ended() {
    let tokens = TokenUsage { input_tokens: 1, output_tokens: 1, cache_read_tokens: 0, cache_write_tokens: 0 };
    let own = EventBody::OwnTurn(turns::OwnTurn::TaskEnded { ended: vec![] });
    let plain = turn_ended(TurnOutcome::Success);
    let mut full = plain.clone();
    if let EventBody::TurnEnded { cost_usd, tokens: told, only_compacted, .. } = &mut full {
        *cost_usd = Some(0.5);
        *told = Some(tokens);
        *only_compacted = true;
    }

    assert_eq!(needs(&plain), []);
    assert_eq!(needs(&own), [Capability::from(turns::Capability::OwnTurns)], "a turn it started by itself");
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
    let asking = |asks| EventBody::from(approval_request("a", asks));

    assert_eq!(
        needs(&EventBody::AgentMode { permission_mode: PermissionMode::Plan }),
        [approvals::Capability::Modes.into()]
    );
    assert_eq!(
        needs(&EventBody::AgentLimits { limits: vec![] }),
        [crate::capability::account::Capability::Limits.into()]
    );
    let plan = approvals::Asks::Plan { plan: "1.".into(), file: None };
    assert_eq!(needs(&asking(plan)), [asking::Capability::PlanApproval.into()]);
    let questions = approvals::Asks::Questions { questions: vec![] };
    assert_eq!(needs(&asking(questions)), [asking::Capability::Questions.into()]);
    assert_eq!(needs(&asked("a")), [approvals::Capability::Requests.into()]);
    assert_eq!(needs(&asking(approvals::Asks::Unknown)), [approvals::Capability::Requests.into()]);
}

#[test]
fn what_the_agent_says_of_its_subagents_needs_them() {
    let subagents = [Capability::from(background::Capability::Subagents)];

    assert_eq!(needs(&subagent_started("t", false)), subagents);
    assert_eq!(needs(&EventBody::SubagentContext { subagent: "t".into(), tokens: 9 }), subagents);
    assert_eq!(needs(&subagent_ended("t")), subagents);
}

#[test]
fn typed_requests_need_their_own_capabilities() {
    let body = approval_request("q", approvals::Asks::Questions { questions: Vec::new() }).into();
    let log = [from_run(1, 1, body)];
    let mut claims = Capabilities::default();
    claims.approvals.requests = Some(approvals::Requests { parts: Vec::new(), kept: Vec::new() });
    assert_eq!(check(&log, |_| &claims), [Breach { law: Law::OnlyWhatsClaimed, seq: Seq::FIRST }]);
    claims.asking.questions = Some(Claimed {});
    assert!(check(&log, |_| &claims).is_empty());
}
