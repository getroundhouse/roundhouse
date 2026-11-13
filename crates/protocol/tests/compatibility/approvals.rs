//! Requests for approval, what they ask and what was decided. The log holds them for good, and peers of any version
//! send decisions, so each form is spelled out.

#![cfg(test)]

use protocol::client::{ClientFrame, ClientRequest};
use protocol::event::{ApprovalDecision, Asking, EventBody, Question, QuestionAnswer, QuestionOption, SessionEvent};
use protocol::frame;

/// What an approval's event says it asks.
fn asking(stored: &[u8]) -> Option<Asking> {
    let event: SessionEvent = frame::decode(stored).unwrap();
    let EventBody::ApprovalRequested { asking, .. } = &event.body else { panic!("{event:?}") };
    assert_eq!(serde_json::to_vec(&event).unwrap(), stored, "and it's written the way it's read");
    asking.as_deref().cloned()
}

/// So a client reads what it asks from its tool and input, as it did before adapters said.
#[test]
fn an_approval_from_before_adapters_said_what_it_asks_says_nothing_and_keeps_its_form() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":1,"place":0,"body":{"type":"approval_requested","approval_id":"a","tool_name":"AskUserQuestion","tool_use_id":"toolu_1","input":{"questions":[{"question":"Red or blue?"}]},"description":null,"suggestions":null}}"#;

    assert_eq!(asking(stored), None);
}

#[test]
fn an_approval_asking_questions_keeps_their_form() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":1,"place":0,"body":{"type":"approval_requested","approval_id":"a","tool_name":"AskUserQuestion","tool_use_id":"toolu_1","input":{},"description":null,"suggestions":null,"asking":{"kind":"questions","questions":[{"text":"Red or blue?","header":"Color","options":[{"label":"Red","description":"Warm"},{"label":"Blue"}],"multiple":true},{"text":"Why?"}]}}}"#;

    let red = QuestionOption { label: "Red".into(), description: Some("Warm".into()) };
    let blue = QuestionOption { label: "Blue".into(), description: None };
    let colors = Question {
        text: "Red or blue?".into(),
        header: Some("Color".into()),
        options: vec![red, blue],
        multiple: true,
    };
    let why = Question { text: "Why?".into(), header: None, options: Vec::new(), multiple: false };
    assert_eq!(asking(stored), Some(Asking::Questions { questions: vec![colors, why] }));
}

#[test]
fn an_approval_asking_for_a_plan_or_a_tools_use_keeps_its_form() {
    let plan = br#"{"session_id":"s","seq":1,"ts_ms":1,"place":0,"body":{"type":"approval_requested","approval_id":"a","tool_name":"ExitPlanMode","tool_use_id":null,"input":{},"description":null,"suggestions":null,"asking":{"kind":"plan","plan":"1. Read"}}}"#;
    let tool = br#"{"session_id":"s","seq":1,"ts_ms":1,"place":0,"body":{"type":"approval_requested","approval_id":"a","tool_name":"Bash","tool_use_id":null,"input":{"command":"ls"},"description":null,"suggestions":null,"asking":{"kind":"tool_use"}}}"#;

    assert_eq!(asking(plan), Some(Asking::Plan { plan: "1. Read".into() }));
    assert_eq!(asking(tool), Some(Asking::ToolUse { call: None }));
}

/// A newer adapter's kind mustn't cost the event, which a client then asks about as a tool's use.
#[test]
fn an_approval_asking_something_this_build_doesnt_know_still_reads() {
    let newer = r#"{"type":"approval_requested","approval_id":"a","tool_name":"Pick","tool_use_id":null,"input":{},"description":null,"asking":{"kind":"a_choice_of_files","files":["a"]}}"#;
    let bare = r#"{"type":"approval_requested","approval_id":"a","tool_name":"AskUserQuestion","tool_use_id":null,"input":{},"description":null,"asking":{"kind":"questions"}}"#;
    let read = |body: &str| match serde_json::from_str(body).unwrap() {
        EventBody::ApprovalRequested { asking, .. } => asking.map(|asking| *asking),
        other => panic!("{other:?}"),
    };

    assert_eq!(read(newer), Some(Asking::Unknown));
    assert_eq!(read(bare), Some(Asking::Questions { questions: Vec::new() }), "one that says no more asks none");
}

/// What a client sends, as a hub reads it, and a worker, which takes the same decision. A client from before sends
/// answers by text, and words of its own for a refusal, so both keep their form.
#[test]
fn every_decision_keeps_its_form() {
    let approve = |decision: &str| {
        let line = format!(
            r#"{{"id":3,"request":{{"method":"session_approve","id":"s","approval_id":"a","decision":{decision}}}}}"#
        );
        let frame: ClientFrame = frame::decode(line.as_bytes()).unwrap();
        let ClientRequest::SessionApprove { decision: read, .. } = &frame.request else { panic!("{frame:?}") };
        assert_eq!(frame::encode(&frame).unwrap(), [line.as_bytes(), b"\n"].concat(), "and it's written as it's read");
        read.clone()
    };
    let by_position = r#"{"decision":"answer_by_position","answers":[{"picked":[0,2],"typed":"Teal"},{}]}"#;
    let by_text = r#"{"decision":"answer","answers":{"Red or blue?":"Blue"}}"#;

    let answers = vec![QuestionAnswer { picked: vec![0, 2], typed: Some("Teal".into()) }, QuestionAnswer::default()];
    assert_eq!(approve(by_position), ApprovalDecision::AnswerByPosition { answers });
    let answers = [("Red or blue?".to_string(), "Blue".to_string())].into();
    assert_eq!(approve(by_text), ApprovalDecision::AnswerByText { answers });
    let said = ApprovalDecision::Deny { message: Some("Not that".into()), interrupt: false };
    assert_eq!(approve(r#"{"decision":"deny","message":"Not that","interrupt":false}"#), said);
    let unsaid = ApprovalDecision::Deny { message: None, interrupt: false };
    assert_eq!(approve(r#"{"decision":"deny","interrupt":false}"#), unsaid, "the adapter's words");
}

#[test]
fn an_approval_closed_with_answers_by_position_keeps_their_form() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":9,"place":0,"body":{"type":"approval_resolved","approval_id":"a","outcome":"allowed","answers_by_position":[{"picked":[1]},{"typed":"Because"},{}]}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    let EventBody::ApprovalResolved { answers_by_text, answers_by_position: Some(answers), .. } = &event.body else {
        panic!("{event:?}")
    };
    let typed = QuestionAnswer { picked: Vec::new(), typed: Some("Because".into()) };
    assert_eq!(*answers, [QuestionAnswer { picked: vec![1], typed: None }, typed, QuestionAnswer::default()]);
    assert_eq!(*answers_by_text, None);
    assert_eq!(serde_json::to_vec(&event).unwrap(), stored, "and it's written the way it's read");
}
