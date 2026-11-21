//! That requests for approval, and what closed them, recorded before either was typed, are typed as they're recorded
//! now.

use protocol::capability::approvals::{Asks, Covers, Decision, Kept, Request, Rule, RuleEffect};
use protocol::capability::asking::{Choose, QuestionAnswer, TypedAnswer};
use protocol::capability::calls::call::{Command, Runs};
use protocol::capability::calls::{Call, CallKind};
use protocol::fixtures::{allow_once, approve_plan, deny, question};
use serde_json::{Value, json};

use super::super::{type_request, type_resolution};

fn bash(command: &str, description: Option<&str>) -> Call {
    Call::Command(Command {
        command: command.into(),
        cwd: None,
        description: description.map(str::to_string),
        timeout_ms: None,
        runs: Runs::Waits,
        parts: Vec::new(),
        unsandboxed: None,
    })
}

/// A request from Claude's adapter to use `tool` with `input`, saying what it asks in `asking`, if it said.
fn claudes(tool: &str, input: Value, asking: Option<Value>) -> Value {
    let mut body =
        json!({"type":"approval_requested","approval_id":"a","tool_name":tool,"tool_use_id":"c","description":null});
    body["input"] = input;
    if let Some(asking) = asking {
        body["asking"] = asking;
    }
    body
}

/// What `body` asks once it's typed, which changes it.
fn typed(mut body: Value) -> Request {
    assert!(type_request(&mut body), "{body}");
    for older in ["tool_name", "input", "description", "suggestions", "asking", "reason"] {
        assert!(body.get(older).is_none(), "{older} is gone from {body}");
    }
    assert_eq!(body["tool_use_id"], "c", "the rest is as it was");
    serde_json::from_value(body["request"].take()).unwrap()
}

#[test]
fn a_request_asks_to_make_its_call_whatever_it_said_of_it() {
    let said_kind = json!({"kind":"tool_use","call":{"kind":"command"}});
    let said_it = json!({"kind":"tool_use","typed":bash("ls -a", None)});
    let call = |call| Asks::Call { call };

    assert_eq!(typed(claudes("Bash", json!({"command":"ls"}), Some(said_kind))).asks, call(bash("ls", None)));
    assert_eq!(typed(claudes("Bash", json!({"command":"ls"}), None)).asks, call(bash("ls", None)), "or nothing");
    assert_eq!(typed(claudes("Bash", json!({"command":"ls"}), Some(said_it))).asks, call(bash("ls -a", None)));
    let mut described = claudes("Bash", json!({"command":"ls"}), None);
    described["description"] = "List it".into();
    assert_eq!(typed(described).asks, call(bash("ls", Some("List it"))), "what it said it's for, as the command's");
    let unknown = json!({"kind":"something_new"});
    assert_eq!(typed(claudes("Bash", json!({"command":"ls"}), Some(unknown))).asks, Asks::Unknown);
}

#[test]
fn questions_and_a_plan_are_read_from_the_tools_input_or_from_what_the_adapter_said() {
    let input = json!({"questions":[
        {"question":"Red or blue?","header":"Color",
         "options":[{"label":"Red","description":"Warm"},{"label":"Blue","preview":"#00f"}]},
        {"question":"Which days?","multiSelect":true,"options":[{"label":"Monday"}]},
        {"header":"No text"},
    ]});
    let Asks::Questions { questions } = typed(claudes("AskUserQuestion", input, None)).asks else { panic!() };
    let said = json!({"kind":"questions","questions":[{"text":"Which days?","multiple":true}]});
    let Asks::Questions { questions: as_said } = typed(claudes("AskUserQuestion", json!({}), Some(said))).asks else {
        panic!()
    };
    let plan = json!({"plan":"1. Read","planFilePath":"/plans/a.md"});

    assert_eq!(questions.len(), 2, "one without its text is left out");
    let red = &questions[0];
    assert_eq!((red.header.as_deref(), red.choose, red.typed), (Some("Color"), Choose::One, TypedAnswer::Yes));
    assert_eq!(red.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Red", "Blue"]);
    assert_eq!(red.options[0].description.as_deref(), Some("Warm"));
    assert_eq!(red.options[1].preview.as_deref(), Some("#00f"));
    assert_eq!(questions[1].choose, Choose::Several);
    assert_eq!((as_said[0].text.as_str(), as_said[0].choose), ("Which days?", Choose::Several));
    let asked_plan = Asks::Plan { plan: "1. Read".into(), file: Some("/plans/a.md".into()) };
    assert_eq!(typed(claudes("ExitPlanMode", plan.clone(), None)).asks, asked_plan);
    let said = json!({"kind":"plan","plan":"1. Read"});
    assert_eq!(typed(claudes("ExitPlanMode", plan, Some(said))).asks, asked_plan);
}

#[test]
fn the_rules_the_cli_suggested_and_why_it_asked_are_the_requests() {
    let mut body = claudes("Bash", json!({"command":"npm test"}), None);
    body["suggestions"] = json!([
        {"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"npm test:*"},{"toolName":"Read"}],
            "behavior":"allow","destination":"localSettings"},
        {"type":"addDirectories","directories":["/tmp"],"destination":"session"},
        {"type":"addRules","rules":[{"ruleContent":"no tool"}],"behavior":"deny","destination":"somewhere"},
    ]);
    body["reason"] = "It runs scripts".into();

    let request = typed(body);

    let rule = |matching: &str, call| Rule {
        covers: Covers::Calls { call },
        matching: matching.into(),
        effect: RuleEffect::Allow,
        kept: Kept::Worktree,
        described: None,
    };
    assert_eq!(request.suggested, [rule("Bash(npm test:*)", CallKind::Command), rule("Read", CallKind::Read)]);
    assert_eq!(request.why.as_deref(), Some("It runs scripts"));
    assert!(!request.always, "nothing said it comes whatever the mode");
    let mut blank = claudes("Bash", json!({"command":"ls"}), None);
    blank["reason"] = " ".into();
    assert_eq!(typed(blank).why, None);
}

#[test]
fn a_typed_request_keeps_it_and_only_the_empty_fields_beside_it_go() {
    let request = json!({"asks":{"asks":"call","call":bash("ls", None)}});
    let mut codex = json!({"type":"approval_requested","approval_id":"a","tool_name":"","input":null,
        "description":null,"request":request});
    let mut since = json!({"type":"approval_requested","approval_id":"a","request":request});
    let kept = since.clone();

    assert!(type_request(&mut codex));
    assert_eq!(codex, kept);
    assert!(!type_request(&mut since), "a request typed already");
    assert_eq!(since, kept);
}

/// What the resolution of a request that asked `asks` stood for, as it was recorded before decisions were, with the
/// call it asked to make denied in the words `said`.
fn decided(mut body: Value, asks: Option<&Asks>, said: Option<&str>) -> Option<Decision> {
    assert!(type_resolution(&mut body, asks, said), "{body}");
    for older in ["outcome", "answers", "answers_by_position"] {
        assert!(body.get(older).is_none(), "{older} is gone from {body}");
    }
    serde_json::from_value(body.get("decision").cloned().unwrap_or(Value::Null)).unwrap()
}

fn resolved(outcome: &str) -> Value {
    json!({"type":"approval_resolved","approval_id":"a","outcome":outcome})
}

#[test]
fn a_use_allowed_was_allowed_once_and_one_denied_in_the_words_its_call_kept() {
    let call = Asks::Call { call: bash("ls", None) };

    assert_eq!(decided(resolved("allowed"), Some(&call), None), Some(allow_once()));
    assert_eq!(decided(resolved("allowed"), None, None), Some(allow_once()), "of a request that isn't there");
    assert_eq!(decided(resolved("denied"), Some(&call), Some("Not on main")), Some(deny(Some("Not on main"))));
    assert_eq!(decided(resolved("denied"), Some(&call), None), Some(deny(None)), "the adapter's words were its own");
    assert_eq!(decided(resolved("cancelled"), Some(&call), None), None, "nobody decided");
    assert_eq!(decided(resolved("something_new"), Some(&call), None), Some(Decision::Unknown));
    let mut since = json!({"type":"approval_resolved","approval_id":"a","decision":allow_once()});
    assert!(!type_resolution(&mut since, Some(&call), None), "typed already");
}

#[test]
fn a_plan_approved_went_on_in_its_conversation_and_one_refused_was_kept_to() {
    let plan = Asks::Plan { plan: "1. Read".into(), file: None };

    assert_eq!(decided(resolved("allowed"), Some(&plan), None), Some(approve_plan()));
    let words = Some("Smaller".to_string());
    assert_eq!(decided(resolved("denied"), Some(&plan), Some("Smaller")), Some(Decision::KeepPlanning { words }));
}

#[test]
fn answers_are_read_by_position_or_by_each_questions_text_and_matched_to_its_options() {
    let several = question("Which days?", &["Monday", "Wednesday", "Friday"], true);
    let asks = Asks::Questions {
        questions: vec![question("Red or blue?", &["Red", "Blue"], false), several, question("Why?", &[], false)],
    };
    let answer =
        |picked: &[u32], typed: Option<&str>| QuestionAnswer { picked: picked.to_vec(), typed: typed.map(Into::into) };
    let answered = |answers| Some(Decision::Answer { answers });

    let mut by_position = resolved("allowed");
    by_position["answers_by_position"] = json!([{"picked":[1]}]);
    assert_eq!(
        decided(by_position, Some(&asks), None),
        answered(vec![answer(&[1], None), QuestionAnswer::default(), QuestionAnswer::default()]),
        "one for each question"
    );
    let mut by_text = resolved("allowed");
    by_text["answers"] = json!({"Red or blue?":"Blue","Which days?":"Monday, Friday","Why?":"Because"});
    assert_eq!(
        decided(by_text, Some(&asks), None),
        answered(vec![answer(&[1], None), answer(&[0, 2], None), answer(&[], Some("Because"))])
    );
    let mut not_theirs = resolved("allowed");
    not_theirs["answers"] = json!({"Red or blue?":"Green","Which days?":"Monday, Sunday"});
    assert_eq!(
        decided(not_theirs, Some(&asks), None),
        answered(vec![answer(&[], Some("Green")), answer(&[], Some("Monday, Sunday")), QuestionAnswer::default()]),
        "what names no option is what was typed"
    );
    let none_answered = answered(vec![QuestionAnswer::default(); 3]);
    assert_eq!(decided(resolved("denied"), Some(&asks), None), none_answered, "skipped, as a skip says it now");
}
