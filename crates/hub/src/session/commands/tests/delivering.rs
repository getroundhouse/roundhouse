//! Where a prompt goes once it's sent: what one the run refuses leaves, and where one goes that a run which has gone
//! wasn't given.

use std::sync::atomic::{AtomicBool, Ordering};

use protocol::capability::Control;
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::{Seq, SessionId};
use protocol::worker::{AgentStarted, WorkerRequest, WorkerResponse};

use super::launching::{Asked, methods};
use crate::hub::tests::{claimed, hub_with_worktree, serving};
use crate::worker_link::WorkerLink;

/// Its run refused the prompt, so it never had it, and says nothing more of the turn the prompt started. The queue
/// waits for the person, as after a failed turn.
#[tokio::test]
async fn a_chat_whose_prompt_its_running_agent_never_had_isnt_left_at_work() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|request| {
        if let WorkerRequest::AgentControl(_) = request {
            return Some(Err(protocol::client::ApiError::new(ErrorCode::Worker, "the agent refused it")));
        }
        let WorkerRequest::AgentStart(spec) = request else { return Some(Ok(WorkerResponse::Ok)) };
        Some(Ok(WorkerResponse::AgentStarted(AgentStarted {
            pid: 7,
            run: spec.run,
            already_running: false,
            capabilities: Some(Box::new(claimed())),
        })))
    });
    serving(&hub, &link);

    hub.session_prompt("s".into(), None, protocol::prompt::Prompt::new("x")).await.unwrap();
    // Once the prompt's delivery is done, since a chat carries out what it's asked in turn.
    let _ = hub.session_cancel("s".into()).await;

    let id = SessionId::from("s");
    let session = hub.blocking(move |store| store.get_session(&id)).await.unwrap().session;
    assert_eq!((session.status, session.queue_held), (protocol::model::SessionStatus::Idle, true));
    let log = hub.blocking(|store| store.read_events(&"s".into(), Seq::FIRST, 100, u64::MAX, false)).await.unwrap();
    assert!(log.iter().any(|event| matches!(event.body, EventBody::PromptDropped { .. })), "{log:?}");
}

/// An agent can exit after the hub last heard it was running and before what's sent to it gets there, as when what it
/// wrote last is still on its way. Its worker says it isn't running, and the hub starts another and sends that to it.
#[tokio::test]
async fn a_prompt_an_agent_that_has_gone_wasnt_given_goes_to_the_next_one() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (kept, gone) = (Asked::default(), AtomicBool::new(false));
    let link = WorkerLink::answering({
        let kept = kept.clone();
        move |request| {
            kept.lock().push(serde_json::to_value(&request).unwrap());
            if matches!(request, WorkerRequest::AgentControl(_)) && !gone.swap(true, Ordering::SeqCst) {
                return Some(Ok(WorkerResponse::AgentNotRunning));
            }
            let WorkerRequest::AgentStart(spec) = request else { return Some(Ok(WorkerResponse::Ok)) };
            Some(Ok(WorkerResponse::AgentStarted(AgentStarted {
                pid: 7,
                run: spec.run,
                already_running: false,
                capabilities: Some(Box::new(claimed())),
            })))
        }
    });
    serving(&hub, &link);
    let live = hub.live_session(&"s".into()).await.unwrap();
    let prompt = Control::Prompt {
        seq: Seq::new(1),
        prompt: protocol::prompt::Prompt::new("x"),
        extras: protocol::capability::prompts::Extras::default(),
    };

    live.send_to_agent(prompt).await.unwrap();

    assert_eq!(methods(&kept), ["agent_start", "agent_control", "agent_start", "agent_control"]);
    let runs: Vec<_> = kept.lock().iter().filter(|r| r["method"] == "agent_start").map(|r| r["run"].clone()).collect();
    assert_ne!(runs[0], runs[1], "a run of its own");
}
