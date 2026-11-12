use client_core::{Answered, Approval};
use protocol::event::{ApprovalOutcome, Asking, EventBody, Question, QuestionAnswer, QuestionOption, SessionEvent};
use protocol::model::Place;
use serde_json::{Value, json};

use super::{requested, resolved};

fn event(tool: &str, input: Value, asking: Option<Asking>) -> SessionEvent {
    let body = EventBody::ApprovalRequested {
        approval_id: "req_1".into(),
        tool_name: tool.into(),
        tool_use_id: None,
        input,
        description: None,
        suggestions: Value::Null,
        asking: asking.map(Box::new),
    };
    SessionEvent {
        session_id: "s".into(),
        seq: 7,
        ts_ms: 0,
        run: None,
        subagent: None,
        place: Place::default(),
        body,
        raw: None,
    }
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
    let tool = lines(&event("Bash", json!({"command": "ls -la"}), Some(Asking::ToolUse)));

    assert_eq!(
        planned,
        ["PLAN:", "  # Plan", "", "  1. Read", "  roundhouse approve s req_1, or with --deny <what to change>"]
    );
    assert_eq!(tool, ["APPROVAL NEEDED: Bash: ls -la", "  roundhouse approve s req_1"]);
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
