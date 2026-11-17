//! Requests for approval, what they ask and what was decided. The log holds them for good, and clients of any version
//! send decisions, so each form is spelled out.

#![cfg(test)]

use protocol::capability::approvals::{AfterDenial, Asks, Covers, Decision, Kept, Remember, Request, Rule, RuleEffect};
use protocol::capability::asking::{Choose, PlanContext, Question, QuestionAnswer, QuestionOption, TypedAnswer};
use protocol::capability::calls::{Call, CallKind};
use protocol::client::{ClientFrame, ClientRequest, agent};
use protocol::event::{EventBody, SessionEvent};
use protocol::fixtures::typed_command;
use protocol::frame;
use serde_json::{Value, json};

/// The event `body` is, as the log keeps it, which it's read from and written back as.
fn kept(body: Value) -> EventBody {
    let mut stored = json!({"session_id":"s","seq":1,"ts_ms":1,"place":0});
    stored["body"] = body;
    let event: SessionEvent = serde_json::from_value(stored.clone()).unwrap();
    assert_eq!(serde_json::to_value(&event).unwrap(), stored, "and it's written the way it's read");
    event.body
}

/// What the request in `body` asks, with who asks and the call it's for.
fn request(body: Value) -> (Request, Option<String>, Option<String>) {
    let EventBody::ApprovalRequested { request, tool_use_id, asked_by, .. } = kept(body) else { panic!("a request") };
    (*request, tool_use_id.map(|id| id.to_string()), asked_by.map(|id| id.to_string()))
}

/// Why the agent asks, which subagent does, the call it's for, the rule it suggests keeping, and that it comes
/// whatever the mode.
#[test]
fn a_request_to_make_a_call_keeps_its_form() {
    let body = json!({"type":"approval_requested","approval_id":"a",
        "request":{"asks":{"asks":"call","call":{"kind":"command","command":"rm -rf out","runs":"waits"}},
            "suggested":[{"covers":"calls","call":"command","matching":"rm:*","effect":"allow","kept":"worktree",
                "described":"Allow rm"}],
            "always":true,"why":"It can't be checked"},
        "tool_use_id":"toolu_2","asked_by":"toolu_1"});

    let (request, call, asked_by) = request(body);
    let rule = Rule {
        covers: Covers::Calls { call: CallKind::Command },
        matching: "rm:*".into(),
        effect: RuleEffect::Allow,
        kept: Kept::Worktree,
        described: Some("Allow rm".into()),
    };
    let asks = Asks::Call { call: Call::Command(typed_command("rm -rf out")) };
    let why = Some("It can't be checked".into());
    assert_eq!(request, Request { asks, suggested: vec![rule], always: true, why });
    assert_eq!((call.as_deref(), asked_by.as_deref()), (Some("toolu_2"), Some("toolu_1")));
}

#[test]
fn a_request_asking_questions_keeps_their_form() {
    let body = json!({"type":"approval_requested","approval_id":"a",
        "request":{"asks":{"asks":"questions","questions":[
            {"text":"Red or blue?","header":"Color","options":[{"label":"Red","description":"Warm","preview":"#f00"},
                {"label":"Blue"}],"choose":"several","typed":"no"},
            {"text":"Your token?","choose":"one","typed":"secret"}]}}});

    let red = QuestionOption { label: "Red".into(), description: Some("Warm".into()), preview: Some("#f00".into()) };
    let blue = QuestionOption { label: "Blue".into(), description: None, preview: None };
    let colors = Question {
        text: "Red or blue?".into(),
        header: Some("Color".into()),
        options: vec![red, blue],
        choose: Choose::Several,
        typed: TypedAnswer::No,
    };
    let token = Question {
        text: "Your token?".into(),
        header: None,
        options: vec![],
        choose: Choose::One,
        typed: TypedAnswer::Secret,
    };
    assert_eq!(request(body).0.asks, Asks::Questions { questions: vec![colors, token] });
}

#[test]
fn a_request_to_approve_a_plan_keeps_its_form() {
    let body = json!({"type":"approval_requested","approval_id":"a",
        "request":{"asks":{"asks":"plan","plan":"1. Read","file":"/plans/read.md"}}});

    assert_eq!(request(body).0.asks, Asks::Plan { plan: "1. Read".into(), file: Some("/plans/read.md".into()) });
}

/// A newer adapter's kind mustn't cost the event, and nor must one from a hub from before requests were typed, which
/// says what it asks only in forms this build doesn't read. A client can only refuse either.
#[test]
fn a_request_asking_what_this_build_doesnt_know_still_reads() {
    let newer = json!({"type":"approval_requested","approval_id":"a",
        "request":{"asks":{"asks":"a_choice_of_files","files":["a"]}}});
    let before = json!({"type":"approval_requested","approval_id":"a","tool_name":"Bash","tool_use_id":"toolu_1",
        "input":{"command":"ls"},"description":null,"suggestions":null,"asking":{"kind":"tool_use"}});
    let asks = |body| match serde_json::from_value(body).unwrap() {
        EventBody::ApprovalRequested { request, .. } => request.asks,
        other => panic!("{other:?}"),
    };

    assert_eq!(asks(newer), Asks::Unknown);
    assert_eq!(asks(before), Asks::Unknown);
}

/// What a client sends, as a hub reads it, and passes on to a worker, which takes the same decision.
#[test]
fn every_decision_a_client_sends_keeps_its_form() {
    let decide = |decision: Value| {
        let line = json!({"id":3,"request":{"method":"session_decide","id":"s","approval_id":"a","decision":decision}});
        let frame: ClientFrame = serde_json::from_value(line.clone()).unwrap();
        let ClientRequest::SessionDecide(agent::SessionDecide { decision: read, .. }) = &frame.request else {
            panic!("{frame:?}")
        };
        assert_eq!(serde_json::to_value(&frame).unwrap(), line, "and it's written as it's read");
        read.clone()
    };
    assert_eq!(
        decide(json!({"decision":"allow","remember":{"remember":"conversation"}})),
        Decision::Allow { remember: Remember::Conversation }
    );
    assert_eq!(
        decide(json!({"decision":"deny","words":"Not that","then":"stop"})),
        Decision::Deny { words: Some("Not that".into()), then: AfterDenial::Stop, keep: None }
    );
    let answers = vec![QuestionAnswer { picked: vec![0, 2], typed: Some("Teal".into()) }, QuestionAnswer::default()];
    assert_eq!(
        decide(json!({"decision":"answer","answers":[{"picked":[0,2],"typed":"Teal"},{}]})),
        Decision::Answer { answers }
    );
    assert_eq!(
        decide(json!({"decision":"keep_planning","words":"Smaller"})),
        Decision::KeepPlanning { words: Some("Smaller".into()) }
    );
    let approved = Decision::ApprovePlan { context: PlanContext::Kept };
    assert_eq!(decide(json!({"decision":"approve_plan","context":"kept"})), approved);
    // As a hub of a build that named the mode to go on in sends it to a worker of this one, after a rollback.
    let named = json!({"decision":"approve_plan","then":{"unasked":"everything","beyond":"refused"},"context":"kept"});
    assert_eq!(serde_json::from_value::<Decision>(named).unwrap(), approved, "the mode it named is left unread");
}

/// What was decided, which the log keeps, and what the hub's settings decided, rather than the person.
#[test]
fn a_request_closed_with_a_decision_keeps_its_form() {
    let resolved = |body| match kept(body) {
        EventBody::ApprovalResolved { decision, by_setting, .. } => (decision.map(|decision| *decision), by_setting),
        other => panic!("{other:?}"),
    };
    let answered = json!({"type":"approval_resolved","approval_id":"a",
        "decision":{"decision":"answer","answers":[{"picked":[1]},{"typed":"Because"},{}]}});
    let by_setting = json!({"type":"approval_resolved","approval_id":"a",
        "decision":{"decision":"allow","remember":{"remember":"once"}},"by_setting":true});

    let typed = QuestionAnswer { picked: Vec::new(), typed: Some("Because".into()) };
    let answers = vec![QuestionAnswer { picked: vec![1], typed: None }, typed, QuestionAnswer::default()];
    assert_eq!(resolved(answered), (Some(Decision::Answer { answers }), false));
    let allowed = Decision::Allow { remember: Remember::Once };
    assert_eq!(resolved(by_setting), (Some(allowed), true));
}

/// One nobody decided, as a stop cancels it, has no decision.
#[test]
fn a_request_closed_with_no_decision_keeps_its_form() {
    let body = kept(json!({"type":"approval_resolved","approval_id":"a"}));

    assert!(matches!(body, EventBody::ApprovalResolved { decision: None, by_setting: false, .. }), "{body:?}");
}

/// A client from before answers with `session_approve`, which this build reads as a request it doesn't know, so the hub
/// refuses it rather than misreading it.
#[test]
fn an_answer_in_the_form_from_before_typed_decisions_is_not_a_decision() {
    let before = br#"{"id":3,"request":{"method":"session_approve","id":"s","approval_id":"a","decision":{"decision":"allow"}}}"#;

    let frame: ClientFrame = frame::decode(before).unwrap();
    assert_eq!(frame.request, ClientRequest::Unknown);
}
