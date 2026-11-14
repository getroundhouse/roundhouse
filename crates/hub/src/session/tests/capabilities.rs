//! Run initialization is durable, and a replaced run cannot choose the conversation the next launch resumes.
use super::super::{CarriedOver, Item, LiveSession};
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, taking_worker};
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
    {
        let state = live.state.lock();
        let conversation = state.agent_session_id.as_ref().map(protocol::model::AgentSessionId::as_str);
        assert_eq!(conversation, Some("s"), "the next launch doesn't resume what a replaced run said");
        assert!(!state.agent_opened && state.capabilities.is_none(), "nor is a replaced run's opening kept live");
    }

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

#[tokio::test]
async fn a_secret_answer_goes_to_the_agent_and_not_the_log() {
    use protocol::event::{ApprovalDecision, QuestionAnswer};
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (link, asked) = taking_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let mut claims = Capabilities::default();
    claims.asking.questions = Some(Claimed {});
    initialized(&live, run, "c", claims).await;
    let by_position = ApprovalDecision::AnswerByPosition {
        answers: vec![
            QuestionAnswer { typed: Some("synthetic-secret-1".into()), picked: Vec::new() },
            QuestionAnswer { typed: Some("public-1".into()), picked: Vec::new() },
        ],
    };
    let by_text = ApprovalDecision::AnswerByText {
        answers: [("Secret?".into(), "synthetic-secret-2".into()), ("Public?".into(), "public-2".into())].into(),
    };
    for (id, decision) in [("q1", by_position), ("q2", by_text)] {
        let request: EventBody = serde_json::from_value(serde_json::json!({
            "type":"approval_requested","approval_id":id,
            "request":{"asks":{"asks":"questions","questions":[
                {"text":"Secret?","typed":"secret"},{"text":"Public?","typed":"yes"}
            ]}}
        }))
        .unwrap();
        live.send_item(Item::Event { run, body: Box::new(request), raw: None, attachments: Vec::new(), seq: None })
            .await;
        flushed(&live).await;
        hub.session_approve(live.id.clone(), id.into(), decision).await.unwrap();
    }

    let id = live.id.clone();
    let log = hub.blocking(move |store| store.read_events(&id, protocol::model::Seq::FIRST, 100, u64::MAX, true));
    let log = serde_json::to_string(&log.await.unwrap()).unwrap();
    let sent = asked.lock().join("\n");
    for n in [1, 2] {
        assert!(log.contains(&format!("public-{n}")), "the rest of the answer is recorded");
        assert!(!log.contains(&format!("synthetic-secret-{n}")), "the log keeps no secret");
        assert!(sent.contains(&format!("synthetic-secret-{n}")), "the agent gets it");
    }
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

    let absent = super::super::decisions::control(&request, "q".into(), &skipped, None);
    let unclaimed = super::super::decisions::control(&request, "q".into(), &skipped, Some(&claims));
    claims.asking.questions = Some(protocol::capability::Claimed {});
    let allowed = super::super::decisions::control(&request, "q".into(), &skipped, Some(&claims)).unwrap();

    assert_eq!(absent.unwrap_err().code, protocol::client::ErrorCode::Unsupported);
    assert_eq!(unclaimed.unwrap_err().code, protocol::client::ErrorCode::Unsupported);
    assert!(matches!(allowed, Some(protocol::capability::Control::Decide {
        decision: protocol::capability::approvals::Decision::Answer { answers }, ..
    }) if answers.is_empty()));
}

#[tokio::test]
async fn a_runs_claims_go_from_the_session_with_the_run_and_not_with_another() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(2);
    let live = carried_on(&hub, &link, run);
    let mut claims = Capabilities::default();
    claims.prompts.steer = Some(Claimed {});
    initialized(&live, run, "s", claims).await;
    let exited = |run| Item::Event {
        run,
        body: Box::new(EventBody::AgentExited {
            reason: protocol::event::ExitReason::Unexpected,
            code: None,
            signal: None,
            stderr_tail: vec![],
        }),
        raw: None,
        attachments: Vec::new(),
        seq: None,
    };
    let on_record = || {
        let id = live.id.clone();
        hub.blocking(move |store| store.get_session(&id))
    };

    live.send_item(exited(RunId::new(1))).await;
    flushed(&live).await;
    assert!(on_record().await.unwrap().session.agent_capabilities.is_some(), "an earlier run's exit leaves them");

    live.send_item(exited(run)).await;
    flushed(&live).await;
    assert!(on_record().await.unwrap().session.agent_capabilities.is_none());
}
