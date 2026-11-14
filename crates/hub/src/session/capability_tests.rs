//! Run initialization is durable, and a replaced run cannot choose the conversation the next launch resumes.
use super::{CarriedOver, Item, LiveSession};
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
use protocol::capability::{Capabilities, Claimed};
use protocol::event::EventBody;
use protocol::model::{RunCapabilities, RunId};
use std::sync::Arc;

async fn initialized(live: &Arc<LiveSession>, run: RunId, conversation: &str, claims: Capabilities) {
    let body = EventBody::AgentInit {
        capabilities: Some(Box::new(claims)),
        agent_session_id: Some(conversation.into()),
        model: None,
        cwd: None,
        permission_mode: None,
        agent_version: None,
    };
    live.send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(live).await;
}

#[tokio::test]
async fn only_the_current_run_can_choose_the_resume_id_and_its_claims_survive_reloading() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(2);
    let live = carried_on(&hub, &link, run);
    initialized(&live, RunId::new(1), "stale", Capabilities::default()).await;
    let id = live.id.clone();
    let untouched = hub.blocking(move |store| store.get_session(&id)).await.unwrap();
    assert!(!untouched.agent_opened);
    assert_eq!(untouched.agent_session_id.as_ref().unwrap().as_str(), "s");
    assert!(untouched.session.agent_capabilities.is_none());

    let mut claims = Capabilities::default();
    claims.prompts.steer = Some(Claimed {});
    initialized(&live, run, "native-chosen", claims.clone()).await;
    let id = live.id.clone();
    let kept = hub.blocking(move |store| store.get_session(&id)).await.unwrap();
    assert!(kept.agent_opened);
    assert_eq!(kept.agent_session_id.as_ref().unwrap().as_str(), "native-chosen");
    assert_eq!(kept.session.agent_capabilities, Some(RunCapabilities { run, claims: Box::new(claims.clone()) }));
    let restored = LiveSession::start(
        &hub,
        kept.clone(),
        &[],
        Some(CarriedOver { link: link.id, run, recorded: None, untaken: Vec::new() }),
    );
    assert_eq!(restored.run_claims(), Some(claims));
    let replaced = LiveSession::start(
        &hub,
        kept,
        &[],
        Some(CarriedOver { link: link.id, run: RunId::new(3), recorded: None, untaken: Vec::new() }),
    );
    assert!(replaced.run_claims().is_none(), "another run cannot inherit old claims");
    let id = live.id.clone();
    assert_eq!(
        hub.blocking(move |store| store.read_events(&id, protocol::model::Seq::FIRST, 100, u64::MAX, false))
            .await
            .unwrap()
            .len(),
        2,
        "stale output remains in history"
    );
}

#[test]
fn secret_answers_go_to_the_agent_but_not_the_recorded_decision() {
    let request: EventBody = serde_json::from_value(serde_json::json!({
        "type":"approval_requested","approval_id":"q",
        "request":{"asks":{"asks":"questions","questions":[
            {"text":"Secret?","typed":"secret"},{"text":"Public?","typed":"yes"}
        ]}}
    }))
    .unwrap();
    let decision = protocol::event::ApprovalDecision::AnswerByPosition {
        answers: vec![
            protocol::event::QuestionAnswer { typed: Some("synthetic-secret".into()), picked: Vec::new() },
            protocol::event::QuestionAnswer { typed: Some("public".into()), picked: Vec::new() },
        ],
    };
    let safe = crate::session::decisions::redacted(&request, &decision);
    let json = serde_json::to_string(&safe).unwrap();
    assert!(!json.contains("synthetic-secret"));
    assert!(json.contains("public"));
    assert!(serde_json::to_string(&decision).unwrap().contains("synthetic-secret"), "the agent still gets the answer");
}

#[test]
fn typed_approval_replies_require_the_runs_claims_even_when_the_answer_is_empty() {
    let request: EventBody = serde_json::from_value(serde_json::json!({
        "type":"approval_requested","approval_id":"q",
        "request":{"asks":{"asks":"questions","questions":[{"text":"Which?","typed":"yes"}]}}
    }))
    .unwrap();
    let skipped = protocol::event::ApprovalDecision::Deny { message: None, interrupt: false };
    let mut claims = protocol::capability::Capabilities::default();

    let absent = super::decisions::control(&request, "q".into(), &skipped, None);
    let unclaimed = super::decisions::control(&request, "q".into(), &skipped, Some(&claims));
    claims.asking.questions = Some(protocol::capability::Claimed {});
    let allowed = super::decisions::control(&request, "q".into(), &skipped, Some(&claims)).unwrap();

    assert_eq!(absent.unwrap_err().code, protocol::client::ErrorCode::Unsupported);
    assert_eq!(unclaimed.unwrap_err().code, protocol::client::ErrorCode::Unsupported);
    assert!(matches!(allowed, Some(protocol::capability::Control::Decide {
        decision: protocol::capability::approvals::Decision::Answer { answers }, ..
    }) if answers.is_empty()));
}
