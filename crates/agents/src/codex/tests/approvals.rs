#![cfg(test)]
use protocol::capability::asking::TypedAnswer;
use serde_json::json;

#[test]
fn native_questions_preserve_choices_open_text_and_secrets() {
    let typed = crate::codex::approvals::typed_answer;
    let choices = json!([{"label":"Blue"}]);
    assert_eq!(typed(&json!({"options":choices})), TypedAnswer::No);
    assert_eq!(typed(&json!({"options":choices,"isOther":false})), TypedAnswer::No);
    assert_eq!(typed(&json!({"options":choices,"isOther":true})), TypedAnswer::Yes);
    assert_eq!(typed(&json!({"options":[]})), TypedAnswer::Yes);
    assert_eq!(typed(&json!({})), TypedAnswer::Yes);
    assert_eq!(typed(&json!({"options":choices,"isSecret":true})), TypedAnswer::Secret);
}

use super::harness::Harness;
use super::script::{bodies, command, item, play, turn};
use crate::{AgentOutput, AgentProcess};
use protocol::capability::{Control, approvals::Decision};
use protocol::event::{ContentBlock, EventBody, TurnOutcome};
use protocol::model::{PermissionMode, Seq, SessionSettingsChange};
use protocol::prompt::Prompt;

#[tokio::test]
async fn keeping_a_plan_restores_read_only_even_after_settings_changed() {
    let mut h = Harness::new(&json!([{"on":"turn/start","actions":[{"result":{"turn":{"id":"planning"}}}]}]));
    let agent = h.started().await;
    crate::codex::approvals::plan(&agent.shared, "Make the change".into()).await;
    let id = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, .. }, .. } = h.next().await {
            break approval_id;
        }
    };
    agent
        .configure(&SessionSettingsChange { permission_mode: Some(PermissionMode::FullAccess), ..Default::default() })
        .await
        .unwrap();
    agent.control(&Control::Decide { approval: id, decision: Decision::KeepPlanning { words: None } }).await.unwrap();
    let request = h.request("turn/start").await;
    assert_eq!(request["params"]["sandboxPolicy"]["type"], "readOnly");
    assert_eq!(request["params"]["collaborationMode"]["mode"], "plan");
    assert_eq!(agent.shared.state.lock().settings.permission_mode, PermissionMode::Plan);
    agent.stop().await;
}

#[tokio::test]
async fn native_request_ids_are_private_to_one_run_and_questions_work_in_both_modes() {
    let mut former = None;
    for mode in [PermissionMode::Plan, PermissionMode::FullAccess] {
        let blocking = mode == PermissionMode::Plan;
        let mut h = Harness::new(&json!([{"on":"turn/start","actions":[
            {"result":{"turn":{"id":"asking"}}},
            {"method":"turn/started","params":{"turn":{"id":"asking"}}},
            {"method":"item/completed","params":{"item":{"type":"userMessage"}}},
            {"id":0,"method":"item/tool/requestUserInput","params":{"isBlocking":blocking,
                "questions":[{"id":"color","question":"Color?","options":[{"label":"Blue"}]}]}}
        ]}]));
        let agent = h.started().await;
        agent.configure(&SessionSettingsChange { permission_mode: Some(mode), ..Default::default() }).await.unwrap();
        agent.send_prompt(Seq::FIRST, &Prompt::new("Ask")).await.unwrap();
        let id = loop {
            if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, request, .. }, .. } =
                h.next().await
            {
                let protocol::capability::approvals::Asks::Questions { questions } = request.unwrap().asks else {
                    panic!("not questions")
                };
                assert_eq!(questions[0].typed, TypedAnswer::No);
                break approval_id;
            }
        };
        assert_ne!(former.as_ref(), Some(&id));
        if let Some(old) = former.take() {
            assert!(
                agent
                    .control(&Control::Decide { approval: old, decision: Decision::Answer { answers: Vec::new() } })
                    .await
                    .is_err()
            );
            assert!(agent.shared.state.lock().waiting.contains_key(&id));
        }
        crate::codex::approvals::resolved(&agent.shared, &json!(0)).await;
        assert!(!agent.shared.state.lock().waiting.contains_key(&id));
        former = Some(id);
        agent.stop().await;
    }
}

#[tokio::test]
async fn an_unrepresentable_effort_cannot_silently_become_the_native_default() {
    let mut h = Harness::new(&json!([]));
    let agent = h.started().await;
    let before = agent.shared.state.lock().settings.clone();
    let settings = protocol::capability::settings::Settings {
        model: Some(protocol::model::ModelName::named("full-model-id")),
        effort: Some(protocol::capability::settings::Effort::Unknown),
        ..Default::default()
    };
    assert!(agent.control(&Control::Change(settings)).await.is_err());
    assert_eq!(agent.shared.state.lock().settings, before);
    agent.stop().await;
}

#[tokio::test]
async fn codex_refusing_the_turn_after_an_approved_plan_ends_the_plans_turn_and_leaves_it_running() {
    use protocol::capability::asking::PlanContext;
    let mut h = Harness::new(&json!([{"on":"turn/start","actions":[{"error":{"code":-32600,"message":"busy"}}]}]));
    let agent = h.started().await;
    crate::codex::approvals::plan(&agent.shared, "Make the change".into()).await;
    let id = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, .. }, .. } = h.next().await {
            break approval_id;
        }
    };
    let decision = Decision::ApprovePlan { then: crate::codex::claims::full_access(), context: PlanContext::Kept };
    assert!(agent.control(&Control::Decide { approval: id, decision }).await.is_err());
    assert!(!agent.is_gone(), "a refusal proves nothing was started, so there's nothing to stop");
    agent.stop().await;
    let mut ends = Vec::new();
    while let Some(output) = h.rx.recv().await {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, errors, .. }, .. } = output {
            ends.push((outcome, errors));
        }
    }
    assert_eq!(ends, [(TurnOutcome::Error, vec!["busy".to_owned()])], "the plan's turn ends, with why");
}

#[tokio::test]
async fn codex_hears_only_the_answers_each_question_allows() {
    use protocol::capability::asking::Choose;
    use protocol::capability::asking::QuestionAnswer;
    let mut h = Harness::new(&json!([{"on":"turn/start","actions":[
        {"result":{"turn":{"id":"asking"}}},
        {"method":"turn/started","params":{"turn":{"id":"asking"}}},
        {"method":"item/completed","params":{"item":{"type":"userMessage"}}},
        {"id":0,"method":"item/tool/requestUserInput","params":{"questions":[
            {"id":"color","question":"Color?","options":[{"label":"Blue"},{"label":"Red"}]},
            {"id":"name","question":"Name?","options":[{"label":"Ann"}],"isOther":true}
        ]}}
    ]}]));
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("Ask")).await.unwrap();
    let id = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, request, .. }, .. } =
            h.next().await
        {
            let protocol::capability::approvals::Asks::Questions { questions } = request.unwrap().asks else {
                panic!("not questions")
            };
            assert!(questions.iter().all(|question| question.choose == Choose::One));
            break approval_id;
        }
    };
    // Typed words for a question with only choices, and a second pick where one is asked for, go nowhere.
    let answers = vec![
        QuestionAnswer { picked: vec![1, 0], typed: Some("Green".into()) },
        QuestionAnswer { picked: Vec::new(), typed: Some("Bo".into()) },
    ];
    agent.control(&Control::Decide { approval: id, decision: Decision::Answer { answers } }).await.unwrap();
    agent.shared.rpc.write(&json!({"method":"test/barrier"})).await.unwrap();
    let mut answered = None;
    loop {
        if let AgentOutput::Event { body: EventBody::AgentOther { kind, .. }, raw: Some(raw), .. } = h.next().await
            && kind == "test/request"
        {
            let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
            if value["params"]["method"] == "test/barrier" {
                break;
            }
            if value["params"]["id"] == 0 {
                answered = Some(value["params"]["result"]["answers"].clone());
            }
        }
    }
    assert_eq!(answered, Some(json!({"color":{"answers":["Red"]},"name":{"answers":["Bo"]}})));
    agent.stop().await;
}

/// A request to make a call names the call, as Claude's do, so a client ties what the call's result says to it, such as
/// that the person denied it.
#[tokio::test]
async fn a_request_to_make_a_call_names_the_call() {
    let asked = |id: u64, kind: &str, call: &str| {
        json!({"id":id,"method":format!("item/{kind}/requestApproval"),
            "params":{"threadId":"thread","turnId":"root-turn","itemId":call}})
    };
    let declined = json!({"type":"commandExecution","id":"run-tests","command":"echo hello","cwd":".",
        "commandActions":[],"status":"declined","aggregatedOutput":null,"exitCode":null});
    let edit = |status: &str| {
        json!({"type":"fileChange","id":"add-notes","status":status,
            "changes":[{"path":"NOTES.md","diff":"hello\n","kind":{"type":"add"}}]})
    };
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        command("thread", "root-turn", "run-tests", false),
        asked(90, "commandExecution", "run-tests"),
        item("thread", "root-turn", declined, true),
        item("thread", "root-turn", edit("inProgress"), false),
        asked(91, "fileChange", "add-notes"),
        item("thread", "root-turn", edit("completed"), true),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    let bodies = bodies(&output);
    let named: Vec<_> = bodies
        .iter()
        .filter_map(|body| {
            if let EventBody::ApprovalRequested { tool_use_id, .. } = body { Some(tool_use_id.clone()) } else { None }
        })
        .collect();
    assert_eq!(named, [Some("run-tests".into()), Some("add-notes".into())]);
    let denied = bodies.iter().find_map(|body| {
        let EventBody::AgentUser { blocks, .. } = body else { return None };
        blocks.iter().find_map(|block| {
            let ContentBlock::CallResult { tool_use_id, denied, .. } = block else { return None };
            (tool_use_id.as_str() == "run-tests").then_some(denied.is_some())
        })
    });
    assert_eq!(denied, Some(true), "the declined call's result, which the request names, says it was denied");
}
