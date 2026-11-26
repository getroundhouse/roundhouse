//! Having a chat's agent connect again to one of its tool servers because someone asked, from a client. Each on tokio's
//! clock, paused, so that nothing more is asked is known once the clock moves.

use std::sync::Arc;

use protocol::capability::Claimed;
use protocol::capability::setup::ServerState;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::RunId;
use protocol::worker::{TOOLS_SERVER, WorkerResponse};
use tokio::sync::oneshot;

use super::launching::current_run;
use super::reconnecting::{Answered, Worker, claims_reconnecting, says_of, settled};
use crate::hub::Hub;
use crate::hub::tests::{hub_with_worktree, tool_servers_of};
use crate::session::{Item, LiveSession};
use crate::tool_servers::tests::signed_in;

fn claims_reloading() -> protocol::capability::Capabilities {
    let mut claims = claims_reconnecting();
    claims.setup.tool_server_reload = Some(Claimed {});
    claims
}

/// A hub, its stand-in worker, whose agent can connect to a server again however it is, and the chat, with its run.
struct Running {
    hub: Arc<Hub>,
    worker: Worker,
    live: Arc<LiveSession>,
    run: RunId,
    _dir: tempfile::TempDir,
}

impl Running {
    async fn new() -> Self {
        Self::claiming(claims_reloading(), || Ok(WorkerResponse::Ok)).await
    }

    async fn claiming(
        claims: protocol::capability::Capabilities,
        answers: impl Fn() -> Answered + Send + 'static,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let worker = Worker::serving(&hub, claims, answers).await;
        let live = hub.live_session(&"s".into()).await.unwrap();
        live.ensure_agent().await.unwrap();
        let run = current_run(&live);
        Self { hub, worker, live, run, _dir: dir }
    }

    /// Its run says the one server it has, which it knows as `name`, is `state`.
    async fn says(&self, name: &str, state: ServerState) {
        says_of(&self.live, self.run, Some("t"), name, state).await;
    }

    /// What a client asking for the server the run has as `name` is answered, once all that comes of it has.
    async fn reload(&self, name: &str) -> Result<(), ErrorCode> {
        let answer = self.asked(name).await.unwrap();
        settled(&self.live).await;
        answer.map_err(|e| e.code)
    }

    /// The same, as it's asked for, while the test goes on.
    fn asked(&self, name: &str) -> tokio::task::JoinHandle<Result<(), ApiError>> {
        let (hub, name) = (Arc::clone(&self.hub), name.into());
        tokio::spawn(async move { hub.session_reload_tool_server("s".into(), name).await })
    }

    /// What it's been asked since it launched, but for the sets of the servers.
    fn told(&self) -> Vec<String> {
        self.worker.told().into_iter().filter(|told| told != "set").collect()
    }
}

#[tokio::test(start_paused = true)]
async fn a_server_is_connected_to_again_only_where_the_run_says_it_has_it_and_claims_it_can_be_as_it_is() {
    let (mut reconnects, mut neither) = (claims_reloading(), claims_reloading());
    reconnects.setup.tool_server_reload = None;
    (neither.setup.tool_server_reload, neither.setup.tool_server_reconnect) = (None, None);
    let (all, reload, connect) = (claims_reloading(), "reload Linear", "Linear");
    let cases = [
        ("one that's connected", &all, connect, ServerState::Connected, Ok(reload)),
        ("one that failed", &all, connect, ServerState::Failed, Ok(connect)),
        ("one that needs signing in to", &all, connect, ServerState::NeedsSignIn, Ok(connect)),
        ("one that's starting", &all, connect, ServerState::Starting, Err(ErrorCode::Conflict)),
        ("one that's turned off", &all, connect, ServerState::Disabled, Err(ErrorCode::Conflict)),
        ("one nothing is known of", &all, connect, ServerState::Unknown, Err(ErrorCode::Conflict)),
        ("one the run doesn't have", &all, "Notion", ServerState::Failed, Err(ErrorCode::NotFound)),
        ("Roundhouse's own", &all, TOOLS_SERVER, ServerState::Failed, Err(ErrorCode::InvalidRequest)),
        ("Roundhouse's own, connected", &all, TOOLS_SERVER, ServerState::Connected, Err(ErrorCode::InvalidRequest)),
        (
            "a run that leaves a connected one",
            &reconnects,
            connect,
            ServerState::Connected,
            Err(ErrorCode::InvalidRequest),
        ),
        ("and that tries one that failed", &reconnects, connect, ServerState::Failed, Ok(connect)),
        ("an older worker's run", &neither, connect, ServerState::Connected, Err(ErrorCode::InvalidRequest)),
        ("and one that failed there", &neither, connect, ServerState::Failed, Err(ErrorCode::InvalidRequest)),
    ];
    for (case, claims, name, state, comes_to) in cases {
        let running = Running::claiming(claims.clone(), || Ok(WorkerResponse::Ok)).await;
        running.says(if name == "Notion" { connect } else { name }, state).await;

        let answer = running.reload(name).await;

        assert_eq!(answer, comes_to.map(drop), "{case}");
        assert_eq!(running.told(), comes_to.into_iter().collect::<Vec<_>>(), "{case}");
    }
}

/// The run's word that it's starting comes apart from its worker's answer, so a second is asked for with neither in, with
/// the answer in, and once it's starting.
#[tokio::test(start_paused = true)]
async fn a_server_asked_for_twice_is_connected_to_again_once_and_again_once_the_run_has_said_what_came_of_it() {
    let running = Running::new().await;
    running.says("Linear", ServerState::Connected).await;

    let (answer, held) = oneshot::channel();
    *running.worker.next_control.lock() = Some(held);
    let (one, other) = (running.asked("Linear"), running.asked("Linear"));
    settled(&running.live).await;
    assert_eq!(running.told(), ["reload Linear"], "two clients at once");
    answer.send(Ok(WorkerResponse::Ok)).unwrap();
    assert_eq!((one.await.unwrap(), other.await.unwrap()), (Ok(()), Ok(())), "each was asked for");
    assert_eq!(running.reload("Linear").await, Ok(()), "one client twice");
    running.says("Linear", ServerState::Starting).await;
    assert_eq!(running.reload("Linear").await, Err(ErrorCode::Conflict));
    assert_eq!(running.told(), ["reload Linear"]);

    running.says("Linear", ServerState::Failed).await;
    assert_eq!(running.reload("Linear").await, Ok(()));
    assert_eq!(running.told(), ["reload Linear", "Linear"]);
}

/// Each leaves nothing that would keep it from being asked for again.
#[tokio::test(start_paused = true)]
async fn a_run_that_has_gone_or_refuses_is_said_to_whoever_asked_and_never_as_a_request_the_hub_doesnt_know() {
    type Answers = fn() -> Answered;
    let has_gone: Answers = || Ok(WorkerResponse::AgentNotRunning);
    let refuses: Answers = || Err(ApiError::new(ErrorCode::Unsupported, "the run does not offer this control"));
    for (answers, code) in [(has_gone, ErrorCode::Conflict), (refuses, ErrorCode::Worker)] {
        let running = Running::claiming(claims_reloading(), answers).await;
        running.says("Linear", ServerState::Failed).await;

        assert_eq!(running.reload("Linear").await, Err(code));
        assert_eq!(running.reload("Linear").await, Err(code));

        assert_eq!(running.told(), ["Linear", "Linear"]);
    }
}

#[tokio::test(start_paused = true)]
async fn no_run_is_started_for_a_reload() {
    let running = Running::new().await;
    running.says("Linear", ServerState::Failed).await;
    let reason = ExitReason::Unexpected;
    let body = Box::new(EventBody::AgentExited { reason, code: Some(0), signal: None, stderr_tail: Vec::new() });
    let exited = Item::Event { run: running.run, body, raw: None, attachments: Vec::new(), seq: None };
    running.live.send_item(exited).await;
    settled(&running.live).await;

    assert_eq!(running.reload("Linear").await, Err(ErrorCode::Conflict));

    assert_eq!(running.told(), Vec::<String>::new());
    assert!(!running.live.agent_running());
}

/// What an earlier run was asked says nothing of the one after it, which has the server again.
#[tokio::test(start_paused = true)]
async fn the_run_after_one_that_was_asked_is_asked_for_itself() {
    let running = Running::new().await;
    running.says("Linear", ServerState::Failed).await;
    assert_eq!(running.reload("Linear").await, Ok(()));
    running.live.let_go_of_agent_on(running.worker.link.id);
    running.live.ensure_agent().await.unwrap();
    let next = current_run(&running.live);
    assert_ne!(next, running.run);

    says_of(&running.live, next, Some("t"), "Linear", ServerState::Failed).await;
    assert_eq!(running.reload("Linear").await, Ok(()));

    assert_eq!(running.told(), ["Linear", "Linear"]);
}

/// The person has signed in to the server as it's asked for, so it's connected to by that sign-in, and isn't again.
#[tokio::test(start_paused = true)]
async fn a_reload_is_also_the_connecting_again_that_a_sign_in_before_it_is_due() {
    let running = Running::new().await;
    running.says("Linear", ServerState::Failed).await;
    // Held, so what's asked for next waits, and the sign-in is done before it's carried out.
    let (answer, held) = oneshot::channel();
    *running.worker.next_control.lock() = Some(held);
    let first = running.asked("Linear");
    settled(&running.live).await;
    running.says("Linear", ServerState::Starting).await;
    running.says("Linear", ServerState::Failed).await;
    let second = running.asked("Linear");
    settled(&running.live).await;
    signed_in(tool_servers_of(&running.hub), "t", "Linear");
    settled(&running.live).await;

    answer.send(Ok(WorkerResponse::Ok)).unwrap();
    assert_eq!((first.await.unwrap(), second.await.unwrap()), (Ok(()), Ok(())));
    settled(&running.live).await;
    running.says("Linear", ServerState::Starting).await;
    running.says("Linear", ServerState::Failed).await;

    assert_eq!(running.told(), ["Linear", "Linear"], "once for each time it was asked for, and none for the sign-in");
}

/// It was asked for before its worker had the token, so it's asked for once more, but not before it has failed without it.
#[tokio::test(start_paused = true)]
async fn a_server_signed_in_to_as_its_being_connected_to_again_is_asked_for_once_that_has_failed() {
    let running = Running::new().await;
    running.says("Linear", ServerState::Failed).await;
    assert_eq!(running.reload("Linear").await, Ok(()));

    signed_in(tool_servers_of(&running.hub), "t", "Linear");
    settled(&running.live).await;
    assert_eq!(running.told(), ["Linear"]);
    running.says("Linear", ServerState::Starting).await;
    assert_eq!(running.told(), ["Linear"]);
    running.says("Linear", ServerState::Failed).await;

    assert_eq!(running.told(), ["Linear", "Linear"]);
}
