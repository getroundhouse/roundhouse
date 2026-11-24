//! Run initialization is durable, and a replaced run cannot choose the conversation the next launch resumes.
use super::super::decisions::refusal;
use super::super::{CarriedOver, Item, LiveSession, Starting};
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, taking_worker};
use protocol::capability::approvals::{AfterDenial, Covers, Decision, Kept, Request, Requests, Rule, RuleEffect};
use protocol::capability::asking::{Question, QuestionAnswer, TypedAnswer};
use protocol::capability::calls::CallKind;
use protocol::capability::{Capabilities, Claimed};
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::fixtures::{approval_request, asking, deny, question, started_claiming, to_run};
use protocol::model::{RunCapabilities, RunId};
use std::sync::Arc;

/// `run` starts, claiming `claims`, and opens the conversation `conversation`.
async fn initialized(live: &Arc<LiveSession>, run: RunId, conversation: &str, claims: Capabilities) {
    let init = EventBody::AgentInit {
        agent_session_id: Some(conversation.into()),
        model: None,
        cwd: None,
        mode: None,
        agent_version: None,
    };
    for body in [started_claiming(claims), init] {
        live.send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None }).await;
    }
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
        Starting::of(kept.clone()),
        Some(CarriedOver { link: link.id, run, recorded: None, untaken: Vec::new() }),
    );
    assert_eq!(restored.run_claims(), Some(claims));
    let replaced = LiveSession::start(
        &hub,
        Starting::of(kept),
        Some(CarriedOver { link: link.id, run: RunId::new(3), recorded: None, untaken: Vec::new() }),
    );
    assert!(replaced.run_claims().is_none(), "another run cannot inherit old claims");
    let id = live.id.clone();
    assert_eq!(
        hub.blocking(move |store| store.read_events(&id, protocol::model::Seq::FIRST, 100, u64::MAX, false))
            .await
            .unwrap()
            .len(),
        4,
        "stale output remains in history, each run's start and initialization"
    );
}

#[tokio::test]
async fn a_secret_answer_goes_to_the_agent_and_not_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (link, asked) = taking_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let mut claims = Capabilities::default();
    claims.asking.questions = Some(Claimed {});
    initialized(&live, run, "c", claims).await;
    let typed = |typed: &str| QuestionAnswer { typed: Some(typed.into()), picked: Vec::new() };
    let answers = Decision::Answer { answers: vec![typed("synthetic-secret"), typed("public")] };
    let secret = Question { typed: TypedAnswer::Secret, ..question("Secret?", &[], false) };
    let request = approval_request("q", asking(vec![secret, question("Public?", &[], false)]));
    live.send_item(Item::Event { run, body: Box::new(request.into()), raw: None, attachments: Vec::new(), seq: None })
        .await;
    flushed(&live).await;
    hub.session_decide(live.id.clone(), "q".into(), answers).await.unwrap();

    let id = live.id.clone();
    let log = hub.blocking(move |store| store.read_events(&id, protocol::model::Seq::FIRST, 100, u64::MAX, true));
    let log = serde_json::to_string(&log.await.unwrap()).unwrap();
    let sent = asked.lock().join("\n");
    assert!(log.contains("public"), "the rest of the answer is recorded");
    assert!(!log.contains("synthetic-secret"), "the log keeps no secret");
    assert!(sent.contains("synthetic-secret"), "the agent gets it");
}

/// A run that says what it claims takes what they allow, and one that has said nothing takes nothing.
#[test]
fn a_decision_goes_only_to_a_run_that_takes_it() {
    let rule = Rule {
        covers: Covers::Calls { call: CallKind::Command },
        matching: "Bash".into(),
        effect: RuleEffect::Deny,
        kept: Kept::Project,
        described: None,
    };
    let request = Request { suggested: vec![rule.clone()], ..approval_request("c", to_run("ls")).request };
    let denied = deny(None);
    let refused =
        |decision: &Decision, claims: Option<&Capabilities>| refusal(&request, decision, claims).map(|e| e.code);
    let mut claims = Capabilities::default();

    assert_eq!(refused(&denied, Some(&claims)), Some(ErrorCode::Unsupported), "none claimed");
    claims.approvals.requests = Some(Requests { parts: Vec::new(), kept: Vec::new() });
    assert_eq!(refused(&denied, Some(&claims)), None);
    assert_eq!(refused(&denied, None), Some(ErrorCode::Unsupported), "none said");
    let newer = Decision::Unknown;
    assert_eq!(refused(&newer, Some(&claims)), Some(ErrorCode::Unsupported), "from a newer client");
    let stopping = Decision::Deny { words: None, then: AfterDenial::Stop, keep: None };
    assert_eq!(refused(&stopping, Some(&claims)), Some(ErrorCode::Unsupported), "a part unclaimed");
    let keeping = Decision::Deny { words: None, then: AfterDenial::Continue, keep: Some(rule) };
    assert!(keeping.answers(&request));
    assert_eq!(refused(&keeping, Some(&claims)), Some(ErrorCode::Unsupported), "a rule to keep, unclaimed");
}

#[test]
fn a_decision_that_doesnt_answer_what_was_asked_is_refused() {
    let only_an_option = Question { typed: TypedAnswer::No, ..question("Which?", &["A"], false) };
    let request = approval_request("q", asking(vec![only_an_option])).request;
    let mut claims = Capabilities::default();
    claims.asking.questions = Some(Claimed {});
    let decide = |answers| refusal(&request, &Decision::Answer { answers }, Some(&claims));
    let answering = |picked, typed: Option<&str>| vec![QuestionAnswer { picked, typed: typed.map(str::to_string) }];

    assert!(decide(answering(vec![0], None)).is_none());
    for (refused, why) in [
        (answering(vec![1], None), "an option it doesn't offer"),
        (answering(vec![], Some("Mine")), "typed where only an option can be chosen"),
        (vec![], "no answer for its question"),
    ] {
        assert_eq!(decide(refused).unwrap().code, ErrorCode::InvalidRequest, "{why}");
    }
}

/// Once the agent that asked has exited, an answer to what it asked is refused as too late, even to a request that
/// only the claims of a run that's running could say how to answer.
#[tokio::test]
async fn an_answer_once_the_agent_that_asked_has_exited_is_refused_as_too_late() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (link, _asked) = taking_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let mut claims = Capabilities::default();
    claims.asking.questions = Some(Claimed {});
    initialized(&live, run, "c", claims).await;
    let request = approval_request("q", asking(vec![question("Which?", &[], false)])).into();
    let exited = EventBody::AgentExited {
        reason: protocol::event::ExitReason::Unexpected,
        code: None,
        signal: None,
        stderr_tail: vec![],
    };
    for body in [request, exited] {
        live.send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None }).await;
    }
    flushed(&live).await;

    let refused = hub.session_decide(live.id.clone(), "q".into(), deny(None)).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Conflict, "{}", refused.message);
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
