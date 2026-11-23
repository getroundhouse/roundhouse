//! A launch, the claims the run it starts is held to, and how the session's record keeps them.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::capability::settings::{Effort, Settings};
use protocol::capability::{Capabilities, Claimed, Control};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::EventBody;
use protocol::model::{ModelName, Rev, RunCapabilities, RunId, Seq, SessionId};
use protocol::worker::{AgentStarted, Delivery, WorkerRequest, WorkerResponse};
use serde_json::Value;

use crate::hub::Hub;
use crate::hub::tests::{claimed, flushed, hub_with_worktree, serving};
use crate::session::{Item, LiveSession, Origin};
use crate::worker_link::WorkerLink;

/// What a stand-in for a worker has been asked, as JSON, in order.
pub(super) type Asked = Arc<Mutex<Vec<Value>>>;

/// A stand-in for `w`'s worker, whose launch claims `claims`, and finds `found` running if it's given, and which takes
/// everything else it's asked.
fn worker(hub: &Hub, found: Option<RunId>, claims: Capabilities) -> (Arc<WorkerLink>, Asked) {
    let asked = Asked::default();
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        kept.lock().push(serde_json::to_value(&request).unwrap());
        let WorkerRequest::AgentStart(spec) = request else { return Some(Ok(WorkerResponse::Ok)) };
        Some(Ok(WorkerResponse::AgentStarted(AgentStarted {
            pid: 7,
            run: found.unwrap_or(spec.run),
            already_running: found.is_some(),
            capabilities: Box::new(claims.clone()),
        })))
    });
    serving(hub, &link);
    (link, asked)
}

/// The agent's requests among what `asked` says, by their methods.
pub(super) fn methods(asked: &Asked) -> Vec<String> {
    let asked = asked.lock();
    let methods = asked.iter().filter_map(|request| request["method"].as_str());
    methods.filter(|method| method.starts_with("agent_")).map(str::to_string).collect()
}

/// The launch among what `asked` says.
fn launch(asked: &Asked) -> Value {
    let asked = asked.lock();
    asked.iter().find(|request| request["method"] == "agent_start").expect("a launch").clone()
}

async fn on_record(hub: &Arc<Hub>) -> Option<RunCapabilities> {
    let id = SessionId::from("s");
    hub.blocking(move |store| store.get_session(&id)).await.unwrap().session.agent_capabilities
}

async fn session_rev(hub: &Arc<Hub>) -> Rev {
    hub.blocking(|store| store.get_session(&"s".into())).await.unwrap().session.rev
}

fn current_run(live: &LiveSession) -> RunId {
    live.state.lock().agent.expect("a run is current").run
}

#[tokio::test]
async fn a_launch_names_the_chats_settings_by_its_kinds_claims_and_holds_the_run_to_what_its_worker_says() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, None, claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();

    live.ensure_agent().await.unwrap();

    let settings: Settings = serde_json::from_value(launch(&asked)["settings"].clone()).unwrap();
    let full_access = claimed().approvals.modes.unwrap().default;
    let named = Settings {
        model: Some(ModelName::named("opus")),
        mode: Some(full_access),
        effort: Some(Effort::Xhigh),
        ..Settings::default()
    };
    assert_eq!(settings, named, "the chat's settings, with its mode as its kind claims it");
    let run = current_run(&live);
    assert_eq!(live.run_claims(), Some(claimed()));
    assert_eq!(on_record(&hub).await, Some(RunCapabilities { run, claims: Box::new(claimed()) }));
    let init = EventBody::AgentInit {
        capabilities: None,
        agent_session_id: Some("c".into()),
        model: None,
        cwd: None,
        mode: None,
        agent_version: None,
    };
    live.send_item(Item::Event { run, body: Box::new(init), raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(&live).await;
    assert_eq!(live.run_claims(), Some(claimed()), "its initialization, which doesn't say them, leaves them");
    assert_eq!(on_record(&hub).await, Some(RunCapabilities { run, claims: Box::new(claimed()) }));
    let log = hub.blocking(|store| store.read_events(&"s".into(), Seq::FIRST, 100, u64::MAX, false)).await.unwrap();
    let started = log.iter().find(|event| matches!(event.body, EventBody::AgentStarted { .. })).unwrap();
    assert_eq!(started.body.claims(), Some(&claimed()), "the log keeps them where the run said them");

    let compact = Control::Compact { asked: Seq::new(1), keeping: None };
    assert_eq!(live.control_agent(&link, compact).await.unwrap(), Delivery::Delivered);
    let refused = live.control_agent(&link, Control::Cancel).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Unsupported, "it doesn't claim cancelling");
    assert_eq!(methods(&asked), ["agent_start", "agent_control"], "nothing it doesn't claim is sent");
}

#[tokio::test]
async fn a_chat_in_a_mode_this_build_has_no_name_for_starts_its_agent_with_full_access() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let unknown = Settings { mode: Some(protocol::capability::approvals::ApprovalMode::UNKNOWN), ..Default::default() };
    hub.blocking(move |store| store.set_session_settings(&"s".into(), &unknown)).await.unwrap();
    let (_link, asked) = worker(&hub, None, claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();

    live.ensure_agent().await.unwrap();

    let settings: Settings = serde_json::from_value(launch(&asked)["settings"].clone()).unwrap();
    assert_eq!(settings.mode, Some(claimed().approvals.modes.unwrap().default));
}

/// Whether the record held them already or not, as when a hub carried on with the run and put them back itself, which
/// is then told to nobody, since the record is as it was.
#[tokio::test]
async fn a_run_its_worker_finds_running_has_its_claims_put_back_on_the_record_and_told_if_that_changed_it() {
    let found = RunId::new(1);
    let kept = RunCapabilities { run: found, claims: Box::new(claimed()) };
    for held in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let (_link, asked) = worker(&hub, Some(found), claimed());
        if held {
            let kept = kept.clone();
            hub.blocking(move |store| store.set_claims(&"s".into(), &kept)).await.unwrap();
        }
        let rev = session_rev(&hub).await;
        let live = hub.live_session(&"s".into()).await.unwrap();
        let mut feed = hub.subscribe_feed();

        live.ensure_agent().await.unwrap();

        assert_eq!(current_run(&live), found);
        assert_eq!(live.run_claims(), Some(claimed()));
        assert_eq!(on_record(&hub).await, Some(kept.clone()));
        assert_eq!(methods(&asked), ["agent_start"]);
        let mut told = Vec::new();
        while let Ok(notice) = feed.try_recv().map(std::sync::Arc::unwrap_or_clone) {
            if let HubNotice::Session(session) = notice {
                told.push(session.agent_capabilities);
            }
        }
        let moved = session_rev(&hub).await != rev;
        if held {
            assert!(!moved, "the record held them already, so its revision stays");
            assert_eq!(told, [], "nothing of the record changed, so nobody is told");
        } else {
            assert!(moved, "putting them on the record is a change");
            assert_eq!(told, [Some(kept.clone())], "every client hears what the run claims");
        }
    }
}

#[tokio::test]
async fn a_run_that_claims_what_needs_a_claim_it_doesnt_make_is_stopped_and_its_launch_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let mut forks = claimed();
    forks.conversation.fork = Some(Claimed {});
    let (_link, asked) = worker(&hub, None, forks);
    let live = hub.live_session(&"s".into()).await.unwrap();

    let Err(refused) = live.ensure_agent().await else { panic!("the launch went ahead") };

    assert_eq!(refused.code, ErrorCode::Worker, "{refused:?}");
    assert_eq!(methods(&asked), ["agent_start", "agent_stop"]);
    assert!(!live.agent_running());
    assert_eq!(on_record(&hub).await, None);
}

/// Whatever asks for the launch, and however late the chat was closed: a prompt on its way to the agent as the chat
/// was closed, past where its delivery looked, or a wait that ended meanwhile, is refused here, under the lock closing
/// takes.
#[tokio::test]
async fn a_closed_chats_agent_is_never_launched() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (_link, asked) = worker(&hub, None, claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();
    live.close().await.unwrap();

    let Err(refused) = live.ensure_agent().await else { panic!("a closed chat's agent was launched") };

    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert_eq!(methods(&asked), ["agent_stop"], "only closing's stop was asked, and no launch");
    assert!(!live.agent_running());
}

#[tokio::test]
async fn a_run_whose_claims_the_session_doesnt_have_is_asked_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, None, claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();

    let none_running = live.control_agent(&link, Control::Cancel).await.unwrap();
    live.ensure_agent().await.unwrap();
    // As for a run its worker said had gone, carried on with before its exit is taken in, whose claims the record
    // didn't keep.
    live.state.lock().capabilities = None;
    let prompt = Control::Prompt {
        seq: Seq::new(1),
        prompt: protocol::prompt::Prompt::new("x"),
        extras: protocol::capability::prompts::Extras::default(),
    };
    let unsaid = live.control_agent(&link, prompt).await.unwrap_err();

    assert_eq!(none_running, Delivery::AgentNotRunning);
    assert_eq!(unsaid.code, ErrorCode::Unsupported);
    assert_eq!(methods(&asked), ["agent_start"]);
}

#[tokio::test]
async fn a_runs_exit_that_takes_its_claims_off_the_record_is_announced_though_the_session_had_moved_on() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, _asked) = worker(&hub, None, claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();
    live.ensure_agent().await.unwrap();
    let run = current_run(&live);
    live.let_go_of_agent_on(link.id);
    let mut feed = hub.subscribe_feed();

    live.send_item(Item::Event { run, body: Box::new(exited()), raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(&live).await;

    assert_eq!(on_record(&hub).await, None);
    let mut told = Vec::new();
    while let Ok(notice) = feed.try_recv().map(std::sync::Arc::unwrap_or_clone) {
        if let HubNotice::Session(session) = notice {
            told.push(session.agent_capabilities);
        }
    }
    assert_eq!(told, [None], "every client hears the chat's claims are its kind's again");
}

#[tokio::test]
async fn a_start_recorded_after_its_runs_exit_puts_no_claims_on_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (_link, _asked) = worker(&hub, None, claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();
    live.ensure_agent().await.unwrap();
    let run = current_run(&live);
    // Its exit was taken in after its launch looked whether it was current, and before its start was recorded.
    live.state.lock().agent = None;
    let id = SessionId::from("s");
    hub.blocking(move |store| store.append(&id, Some(run), &exited(), None, None, None).map(drop)).await.unwrap();

    let capabilities = Some(Box::new(claimed()));
    let started = EventBody::AgentStarted { pid: 7, resumed: false, build: None, previous_build: None, capabilities };
    live.record(Origin::Run { run, current: true, numbered: None }, started, None).await.unwrap();

    assert_eq!(on_record(&hub).await, None);
}

#[tokio::test]
async fn a_run_found_running_that_has_exited_since_gets_no_claims_on_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let found = RunId::new(1);
    let (_link, _asked) = worker(&hub, Some(found), claimed());
    let live = hub.live_session(&"s".into()).await.unwrap();
    live.ensure_agent().await.unwrap();
    let id = SessionId::from("s");
    hub.blocking(move |store| store.append(&id, Some(found), &exited(), None, None, None).map(drop)).await.unwrap();
    // Its exit was taken in after the launch's reply, and before its claims were put back.
    live.state.lock().agent = None;

    live.keep_claims(&hub, RunCapabilities::new(found, claimed())).await;

    assert_eq!(on_record(&hub).await, None);
}

fn exited() -> EventBody {
    EventBody::AgentExited {
        reason: protocol::event::ExitReason::Unexpected,
        code: None,
        signal: None,
        stderr_tail: vec![],
    }
}
