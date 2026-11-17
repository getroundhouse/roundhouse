//! What the migration that typed requests for approval, what closed them, and the drafts of answers to them made of
//! what the hub had kept.

use protocol::capability::approvals::{AfterDenial, Asks, Decision};
use protocol::capability::asking::QuestionAnswer;
use protocol::capability::calls::Call;
use protocol::event::EventBody;
use protocol::fixtures::{allow_once, to_run, typed_command, typed_denied};
use protocol::model::{AnswerSoFar, DecisionDraft, Seq, SessionId, SoFar, ToolUseId, UseChoice};
use serde_json::{Value, json};

use crate::store::{Store, older, schema};

/// A database as the build before the migration left it, with Claude's chat `s` holding `bodies`, each an
/// event's body as JSON, in order, and a draft of an answer, `draft`.
fn migrated(dir: &std::path::Path, bodies: &[Value], draft: Option<&Value>) -> Store {
    let quoted = |json: String| json.replace('\'', "''");
    let draft = draft.map_or_else(|| "NULL".to_string(), |draft| format!("'{}'", quoted(draft.to_string())));
    let chat = format!(
        "INSERT INTO sessions (id, workspace_id, agent, model, permission_mode, effort, agent_session_id, status, \
         last_seq, created_at_ms, updated_at_ms, answer_draft) \
         VALUES ('s', 'w', 'claude', 'opus', 'bypassPermissions', 'high', 's', 'idle', {}, 0, 0, {draft});",
        bodies.len()
    );
    let events = (1..).zip(bodies).map(|(seq, body)| {
        format!(
            "INSERT INTO events (session_id, seq, ts_ms, kind, place, body) VALUES ('s', {seq}, 0, '{}', {seq}, '{}');",
            body["type"].as_str().unwrap(),
            quoted(body.to_string()),
        )
    });
    let rows: String = std::iter::once(chat).chain(events).collect();
    older::store_from_before(dir, schema::BEFORE_TYPED_REQUESTS, &rows)
}

fn bodies(store: &Store) -> Vec<EventBody> {
    let events = store.read_events(&SessionId::from("s"), Seq::new(1), 100, u64::MAX, false).unwrap();
    events.into_iter().map(|event| event.body).collect()
}

/// A request from Claude's adapter, `id`, to run `command` in the call `call`, as the migration that typed its calls
/// left it.
fn to_run_untyped(id: &str, call: &str, command: &str) -> Value {
    let typed = Call::Command(typed_command(command));
    json!({"type":"approval_requested","approval_id":id,"tool_name":"Bash","tool_use_id":call,
        "input":{"command":command},"asking":{"kind":"tool_use","typed":typed}})
}

fn decision(body: &EventBody) -> Option<&Decision> {
    let EventBody::ApprovalResolved { decision, .. } = body else { panic!("{body:?}") };
    decision.as_deref()
}

#[test]
fn each_request_is_typed_and_what_closed_it_is_the_decision_it_stood_for() {
    let dir = tempfile::tempdir().unwrap();
    let questions = json!({"type":"approval_requested","approval_id":"q","tool_name":"AskUserQuestion",
        "input":{"questions":[{"question":"Red or blue?","options":[{"label":"Red"},{"label":"Blue"}]}]}});
    // The person's words come back with the call they denied, after what closed the request.
    let denied_result = json!({"type":"agent_user","blocks":[typed_denied("rm", "Not that", Some("Not on main"))]});
    let log = [
        to_run_untyped("a", "ls", "ls"),
        json!({"type":"approval_resolved","approval_id":"a","outcome":"allowed"}),
        to_run_untyped("b", "rm", "rm -rf out"),
        json!({"type":"approval_resolved","approval_id":"b","outcome":"denied"}),
        denied_result,
        questions,
        json!({"type":"approval_resolved","approval_id":"q","outcome":"allowed","answers":{"Red or blue?":"Blue"}}),
        to_run_untyped("c", "pwd", "pwd"),
        json!({"type":"approval_resolved","approval_id":"c","outcome":"cancelled"}),
    ];

    let store = migrated(dir.path(), &log, None);

    let bodies = bodies(&store);
    let EventBody::ApprovalRequested { request, tool_use_id, .. } = &bodies[0] else { panic!("{:?}", bodies[0]) };
    assert_eq!((&request.asks, tool_use_id.as_ref().map(ToolUseId::as_str)), (&to_run("ls"), Some("ls")));
    assert_eq!(decision(&bodies[1]), Some(&allow_once()));
    let words = Some("Not on main".into());
    assert_eq!(decision(&bodies[3]), Some(&Decision::Deny { words, then: AfterDenial::Continue, keep: None }));
    let EventBody::ApprovalRequested { request, .. } = &bodies[5] else { panic!("{:?}", bodies[5]) };
    assert!(matches!(&request.asks, Asks::Questions { questions } if questions[0].text == "Red or blue?"));
    let blue = vec![QuestionAnswer { picked: vec![1], typed: None }];
    assert_eq!(decision(&bodies[6]), Some(&Decision::Answer { answers: blue }), "by the option it names");
    assert_eq!(decision(&bodies[8]), None, "nobody decided");
}

#[test]
fn a_draft_of_an_answer_is_one_of_a_decision_for_what_its_request_asks() {
    let draft = |id: &str, picked: &[u32], typed: &str, other: bool| json!({"approval_id":id,"answers":[{"picked":picked,"typed":typed,"other":other}]});
    let log = [to_run_untyped("a", "ls", "ls")];
    let questions = json!({"type":"approval_requested","approval_id":"q","tool_name":"AskUserQuestion",
        "input":{"questions":[{"question":"Why?"}]}});
    let plan = json!({"type":"approval_requested","approval_id":"p","tool_name":"ExitPlanMode","input":{"plan":"1."}});
    let kept = |log: &[Value], draft: Value| {
        let dir = tempfile::tempdir().unwrap();
        migrated(dir.path(), log, Some(&draft)).get_session(&"s".into()).unwrap().session.decision_draft
    };

    let use_draft = |chosen, words: &str, writing| DecisionDraft {
        approval_id: "a".into(),
        so_far: SoFar::Use { chosen, words: words.into(), writing },
    };
    assert_eq!(kept(&log, draft("a", &[1], "", false)), Some(use_draft(Some(UseChoice::Deny), "", false)));
    assert_eq!(kept(&log, draft("a", &[], "Use -l", true)), Some(use_draft(None, "Use -l", true)));
    let so_far = AnswerSoFar { picked: Vec::new(), typed: "Because".into(), other: true };
    assert_eq!(
        kept(&[questions], draft("q", &[], "Because", true)),
        Some(DecisionDraft { approval_id: "q".into(), so_far: SoFar::Answers { answers: vec![so_far] } })
    );
    assert_eq!(kept(&[plan], draft("p", &[0], "", false)), None, "no client drafts a plan's approval");
    assert_eq!(kept(&log, draft("gone", &[0], "", false)), None, "nor is a draft kept for a request not asked");
}
