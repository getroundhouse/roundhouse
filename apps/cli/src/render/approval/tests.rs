use client_core::{Answered, Approval};
use protocol::event::{ApprovalOutcome, Asking, CallKind, Question, QuestionAnswer, QuestionOption, SessionEvent};
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::Seq;
use serde_json::{Value, json};

use super::{requested, resolved};

fn event(tool: &str, input: Value, asking: Option<Asking>) -> SessionEvent {
    let body = ApprovalRequest { input, asking, ..approval_request("req_1", tool) };
    protocol::fixtures::event(Seq::new(7), body.into())
}

fn lines(event: &SessionEvent) -> Vec<String> {
    requested(event, &Approval::of(&event.body).unwrap())
}

fn colors() -> Question {
    let red = QuestionOption { label: "Red".into(), description: Some("Warm".into()) };
    let blue = QuestionOption { label: "Blue".into(), description: None };
    Question { text: "Red or blue?".into(), header: Some("Color".into()), options: vec![red, blue], multiple: true }
}

#[test]
fn a_question_prints_its_options_numbered_and_how_to_answer() {
    let asking = Asking::Questions { questions: vec![colors()] };

    let printed = lines(&event("AskUserQuestion", json!({}), Some(asking)));

    assert_eq!(
        printed,
        [
            "QUESTION: Color: Red or blue?",
            "  1. Red: Warm",
            "  2. Blue",
            "  (any number of these)",
            r#"  roundhouse answer s req_1 "Red or blue?" <answer>"#,
        ]
    );
}

#[test]
fn a_plan_prints_whole_and_a_tool_by_its_input() {
    let plan = Asking::Plan { plan: "# Plan\n\n1. Read".into() };

    let planned = lines(&event("ExitPlanMode", json!({}), Some(plan)));
    let tool = lines(&event("Bash", json!({"command": "ls -la"}), Some(Asking::ToolUse { call: None })));

    assert_eq!(
        planned,
        ["PLAN:", "  # Plan", "", "  1. Read", "  roundhouse approve s req_1, or with --deny <what to change>"]
    );
    assert_eq!(tool, ["APPROVAL NEEDED: Bash: ls -la", "  roundhouse approve s req_1"]);
}

#[test]
fn a_tools_use_prints_its_command_whole_under_what_its_for_with_why_and_who_asks() {
    let asked = ApprovalRequest {
        input: json!({"command": "D=\"$(cat path)\"\nrm -rf \"$D\""}),
        description: Some("Remove the scratch directory".into()),
        asking: Some(Asking::ToolUse { call: None }),
        reason: Some("The target is only known once it runs".into()),
        asked_by: Some("toolu_agent".into()),
        ..approval_request("req_1", "Bash")
    };

    let printed = lines(&protocol::fixtures::event(Seq::new(7), asked.into()));

    assert_eq!(
        printed,
        [
            "APPROVAL NEEDED: Bash: Remove the scratch directory",
            r#"  $ D="$(cat path)""#,
            r#"  $ rm -rf "$D""#,
            "  why: The target is only known once it runs",
            "  asked by the subagent toolu_agent started",
            "  roundhouse approve s req_1",
        ]
    );
}

#[test]
fn an_edit_prints_its_file_and_what_it_takes_out_and_puts_in() {
    let input = json!({"file_path": "src/lib.rs", "old_string": "let a = 1;", "new_string": "let a = 2;\nlet b = 3;"});
    let described = ApprovalRequest {
        input: input.clone(),
        description: Some("Fix the numbers".into()),
        asking: Some(Asking::ToolUse { call: Some(CallKind::Edit) }),
        ..approval_request("req_1", "Edit")
    };
    let plain = ApprovalRequest { description: None, ..described.clone() };

    let described = lines(&protocol::fixtures::event(Seq::new(7), described.into()));
    let plain = lines(&protocol::fixtures::event(Seq::new(7), plain.into()));

    let change = ["  - let a = 1;", "  + let a = 2;", "  + let b = 3;", "  roundhouse approve s req_1"];
    assert_eq!(described[..2], ["APPROVAL NEEDED: Edit: Fix the numbers", "  file: src/lib.rs"]);
    assert_eq!(described[2..], change);
    assert_eq!(plain[0], "APPROVAL NEEDED: Edit: src/lib.rs", "its file said once");
    assert_eq!(plain[1..], change);
}

#[test]
fn a_question_from_before_adapters_said_still_prints_as_one() {
    let input = json!({"questions": [{"question": "Why?"}]});

    let printed = lines(&event("AskUserQuestion", input, None));

    assert_eq!(printed[0], "QUESTION: Why?");
}

#[test]
fn what_was_answered_prints_by_question() {
    let why = Question { text: "Why?".into(), header: None, options: Vec::new(), multiple: false };
    let answers = vec![QuestionAnswer { picked: vec![0, 1], typed: None }, QuestionAnswer::default()];

    let printed = resolved("req_1", ApprovalOutcome::Allowed, Some(&Answered::ByPosition(answers)), &[colors(), why]);
    let by_text = Answered::ByText([("Red or blue?".to_string(), "Teal".to_string())].into());
    let unseen = resolved("req_1", ApprovalOutcome::Allowed, Some(&by_text), &[]);

    assert_eq!(printed, ["approval req_1 Allowed", "  Red or blue?: Red, Blue", "  Why?: not answered"]);
    assert_eq!(unseen, ["approval req_1 Allowed", "  Red or blue?: Teal"], "one whose request this didn't print");
    assert_eq!(resolved("req_1", ApprovalOutcome::Denied, None, &[colors()]), ["approval req_1 Denied"]);
}
