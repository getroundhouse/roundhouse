//! What the CLI asks about under full access, as every chat is launched, in its captured output.
//! `../../fixtures/claude_full_access.jsonl` holds two conversations, captured by
//! `tools/smoke/capture_claude_fixture.py --full-access`: a removal the CLI can't rule out as dangerous, asked about
//! for the agent's own command, then for a subagent's.

use agents::claude::{Normalizer, Parsed};
use protocol::event::{ContentBlock, Denied, EventBody};
use protocol::model::{PermissionMode, ToolUseId};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../fixtures/claude_full_access.jsonl");

fn events() -> Vec<EventBody> {
    let mut normalizer = Normalizer::default();
    let parsed = FIXTURE.lines().map(|line| normalizer.line(line).parsed);
    parsed.filter_map(|parsed| if let Parsed::Event(body) = parsed { Some(body) } else { None }).collect()
}

/// Why each request in the fixture says it's made, as the CLI wrote it.
fn reasons_the_cli_gave() -> Vec<String> {
    let lines = FIXTURE.lines().map(|line| serde_json::from_str::<Value>(line).unwrap());
    lines.filter_map(|line| line["request"]["decision_reason"].as_str().map(str::to_string)).collect()
}

#[test]
fn full_access_still_asks_about_a_removal_it_cant_rule_out_and_says_why_and_which_subagent_asks() {
    let events = events();
    let modes = events.iter().filter_map(|e| match e {
        EventBody::AgentInit { permission_mode, .. } => Some(*permission_mode),
        _ => None,
    });
    let started: Vec<&ToolUseId> = events
        .iter()
        .filter_map(|e| if let EventBody::SubagentStarted { call, .. } = e { Some(call) } else { None })
        .collect();
    let asked: Vec<&EventBody> = events.iter().filter(|e| matches!(e, EventBody::ApprovalRequested { .. })).collect();
    let said = |request: &EventBody| match request {
        EventBody::ApprovalRequested { reason, asked_by, .. } => (reason.clone(), asked_by.clone()),
        other => panic!("{other:?}"),
    };

    assert_eq!(modes.collect::<Vec<_>>(), [Some(PermissionMode::FullAccess); 2], "each conversation's launch");
    let [subagent] = started[..] else { panic!("{started:?}") };
    let [own, its] = asked[..] else { panic!("{asked:?}") };
    let [own_reason, its_reason] = &reasons_the_cli_gave()[..] else { panic!("each request says why") };
    assert_eq!(said(own), (Some(own_reason.clone()), None), "the agent's own");
    assert_eq!(said(its), (Some(its_reason.clone()), Some(subagent.clone())), "by the call that started it");
    assert_eq!(its.subagent(), None, "among the agent's own events, where a chat reads it");
}

/// The CLI says beside a call's result that the person rejected it, and the result is what they were told to say.
#[test]
fn each_denied_calls_result_says_it_was_denied_with_what_the_person_said_and_no_other_does() {
    let events = events();
    let results = events.iter().flat_map(|e| match e {
        EventBody::AgentUser { blocks, .. } => blocks.as_slice(),
        _ => &[],
    });
    let marks: Vec<(bool, Option<Denied>)> = results
        .filter_map(|block| match block {
            ContentBlock::ToolResult { is_error, denied, .. } => Some((*is_error, denied.clone())),
            _ => None,
        })
        .collect();

    let denied = || Some(Denied { said: Some("Not allowed".into()) });
    let failed: Vec<&Option<Denied>> = marks.iter().filter(|(failed, _)| *failed).map(|(_, denied)| denied).collect();
    assert_eq!(failed, [&denied(), &denied()], "the agent's own and the subagent's, in the capture's words");
    assert!(marks.iter().filter(|(failed, _)| !failed).all(|(_, denied)| denied.is_none()), "{marks:?}");
}
