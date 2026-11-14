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
use crate::{AgentOutput, AgentProcess};
use protocol::capability::{Control, approvals::Decision};
use protocol::event::EventBody;
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
        effort: Some(protocol::capability::settings::Effort::Persistent),
        ..Default::default()
    };
    assert!(agent.control(&Control::Change(settings)).await.is_err());
    assert_eq!(agent.shared.state.lock().settings, before);
    agent.stop().await;
}
