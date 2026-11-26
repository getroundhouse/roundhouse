//! Having a chat's agent connect again to a tool server it was given, once the person has signed in to it. Each on
//! tokio's clock, paused, so that nothing more is asked is known once the clock moves.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::capability::{Capabilities, Claimed, Control, Report};
use protocol::client::{ApiError, ErrorCode};
use protocol::event::SessionEvent;
use protocol::model::{RunId, Seq, ToolScopes};
use protocol::worker::{
    AgentControl, AgentStarted, AskForToolAccess, TOOLS_SERVER, TokenNumber, ToolAccess, WorkerRequest, WorkerResponse,
};
use tokio::sync::oneshot;

use super::launching::current_run;
use crate::hub::Hub;
use crate::hub::tests::{
    carried_on, claimed, flushed, hub_with_worktree, serving, started_again, taken, tool_servers_of,
};
use crate::session::{Item, LiveSession};
use crate::tool_servers::tests::{again, named, signed_in};
use crate::worker_link::{Answer, WorkerLink};

pub(super) type Answered = Result<WorkerResponse, ApiError>;

/// What an answer waits for, if a test has it wait.
type Held<T> = Arc<Mutex<Option<oneshot::Receiver<T>>>>;

/// A stand-in for `w`'s worker, as the hub's own link to one is once it has opened.
pub(super) struct Worker {
    pub(super) link: Arc<WorkerLink>,
    /// Each set of the tool servers, as `set`, each server its agent was asked to connect to again, by its name, after
    /// `reload` for one it's connected to, and each prompt it was given, as `prompt`, with what a test has said beside
    /// them, in order.
    told: Arc<Mutex<Vec<String>>>,
    /// The run its agent is of already, if it runs.
    running: Arc<Mutex<Option<RunId>>>,
    /// What the next set's answer, the next launch's, or the next of the agent's waits for.
    pub(super) next_set: Held<Answered>,
    next_start: Held<()>,
    pub(super) next_control: Held<Answered>,
}

pub(super) fn claims_reconnecting() -> Capabilities {
    let mut claims = claimed();
    claims.setup.tool_servers = Some(Claimed {});
    claims.setup.tool_server_reconnect = Some(Claimed {});
    claims
}

impl Worker {
    /// Whose agent claims `claims`, and which answers being asked to connect again with `answers`.
    pub(super) async fn serving(
        hub: &Arc<Hub>,
        claims: Capabilities,
        answers: impl Fn() -> Answered + Send + 'static,
    ) -> Self {
        let told = Arc::<Mutex<Vec<String>>>::default();
        let (next_set, next_start) = (Held::<Answered>::default(), Held::<()>::default());
        let next_control = Held::<Answered>::default();
        let (telling, set, start) = (Arc::clone(&told), Arc::clone(&next_set), Arc::clone(&next_start));
        let running = Arc::<Mutex<Option<RunId>>>::default();
        let (control, found) = (Arc::clone(&next_control), Arc::clone(&running));
        let link = WorkerLink::answering_in_time(move |request| {
            if let WorkerRequest::ToolServersSet(_) = request {
                telling.lock().push("set".into());
                let held = set.lock().take();
                return Some(held.map_or(Answer::Now(Ok(WorkerResponse::Ok)), Answer::Later));
            }
            if let WorkerRequest::AgentControl(AgentControl { control: asked, .. }) = &request {
                if let Control::Prompt { .. } = asked {
                    telling.lock().push("prompt".into());
                } else {
                    let reload = matches!(asked, Control::Reload { .. });
                    let (Control::Reconnect { server } | Control::Reload { server }) = asked else {
                        panic!("{asked:?}")
                    };
                    telling.lock().push(if reload { format!("reload {server}") } else { server.to_string() });
                }
                let held = control.lock().take();
                return Some(held.map_or_else(|| Answer::Now(answers()), Answer::Later));
            }
            let WorkerRequest::AgentStart(spec) = request else { return Some(Answer::Now(Ok(taken(&request)))) };
            let running = *found.lock();
            let answer = Ok(WorkerResponse::AgentStarted(AgentStarted {
                pid: 7,
                run: running.unwrap_or(spec.run),
                already_running: running.is_some(),
                capabilities: Box::new(claims.clone()),
            }));
            let waits_for = start.lock().take();
            let Some(waits_for) = waits_for else { return Some(Answer::Now(answer)) };
            let (then, later) = oneshot::channel();
            tokio::spawn(async move {
                let _ = waits_for.await;
                let _ = then.send(answer);
            });
            Some(Answer::Later(later))
        });
        serving(hub, &link);
        link.set_as_opened(hub, "w".into()).await;
        Self { link, told, running, next_set, next_start, next_control }
    }

    /// The same, whose agent claims it can connect again, and does.
    async fn reconnecting(hub: &Arc<Hub>) -> Self {
        Self::serving(hub, claims_reconnecting(), || Ok(WorkerResponse::Ok)).await
    }

    /// The same, whose agent is of `run`, and runs already.
    pub(super) async fn running(hub: &Arc<Hub>, claims: Capabilities, run: RunId) -> Self {
        let worker = Self::serving(hub, claims, || Ok(WorkerResponse::Ok)).await;
        *worker.running.lock() = Some(run);
        worker
    }

    /// That `mark` came after whatever it's been asked.
    pub(super) fn mark(&self, mark: &str) {
        self.told.lock().push(mark.into());
    }

    pub(super) fn told(&self) -> Vec<String> {
        self.told.lock().clone()
    }
}

/// Once everything under way has come to rest.
#[expect(clippy::disallowed_methods, reason = "on tokio's paused clock, which moves once nothing else can")]
pub(super) async fn settled(live: &LiveSession) {
    flushed(live).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
}

/// What `run` says of the server `t`, which it knows as `Linear`, once all that comes of it has.
async fn says(live: &LiveSession, run: RunId, state: ServerState) {
    says_of(live, run, Some("t"), "Linear", state).await;
}

/// The same, of the one it knows as `name`, which is `hubs` of the hub's if it's any, as its worker says.
pub(super) async fn says_of(live: &LiveSession, run: RunId, hubs: Option<&str>, name: &str, state: ServerState) {
    let server = ToolServerState {
        name: name.into(),
        hubs: hubs.map(Into::into),
        state,
        error: None,
        tools: None,
        connects_again: None,
    };
    says_server(live, run, server).await;
}

/// The same, as `server` says.
pub(super) async fn says_server(live: &LiveSession, run: RunId, server: ToolServerState) {
    live.send_item(Item::Report { run, report: Report::ToolServers { servers: vec![server] }, seq: None }).await;
    settled(live).await;
}

async fn log(hub: &Arc<Hub>) -> Vec<SessionEvent> {
    hub.blocking(|store| store.read_events(&"s".into(), Seq::FIRST, 100, u64::MAX, false)).await.unwrap()
}

/// The server has been renamed since the agent launched, which goes on knowing it as it did.
#[tokio::test(start_paused = true)]
async fn an_agent_is_asked_once_for_each_sign_in_only_once_its_worker_has_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let servers = tool_servers_of(&hub);
    signed_in(servers, "t", "Linear");
    let worker = Worker::reconnecting(&hub).await;
    let live = hub.live_session(&"s".into()).await.unwrap();
    live.ensure_agent().await.unwrap();
    let run = current_run(&live);
    says(&live, run, ServerState::Failed).await;
    assert_eq!(worker.told(), ["set"], "as it launched with it");
    servers.edit(named("t", "Tickets")).unwrap();
    settled(&live).await;

    let (answer, held) = oneshot::channel();
    *worker.next_set.lock() = Some(held);
    again(servers, "t").unwrap();
    says(&live, run, ServerState::Failed).await;
    worker.mark("has the token");
    answer.send(Ok(WorkerResponse::Ok)).unwrap();
    settled(&live).await;
    says(&live, run, ServerState::Failed).await;

    assert_eq!(worker.told(), ["set", "set", "set", "has the token", "Linear"]);
}

/// Each has an agent that can say a server failed when the person has signed in to another, and nothing to ask of it.
#[tokio::test(start_paused = true)]
async fn an_agent_is_asked_only_if_it_claims_it_can_and_was_given_the_server_and_never_for_roundhouses_own() {
    let mut earlier = claims_reconnecting();
    earlier.setup.tool_server_reconnect = None;
    let cases = [
        ("a run that doesn't claim it", earlier, Some("t"), "Linear"),
        ("a server added after the launch, and the CLI's own of its name", claims_reconnecting(), None, "Linear"),
        ("Roundhouse's own", claims_reconnecting(), Some("t"), TOOLS_SERVER),
        ("another of the hub's, which isn't signed in to", claims_reconnecting(), Some("u"), "Linear"),
    ];
    for (case, claims, hubs, name) in cases {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let worker = Worker::serving(&hub, claims, || Ok(WorkerResponse::Ok)).await;
        let live = hub.live_session(&"s".into()).await.unwrap();
        live.ensure_agent().await.unwrap();

        signed_in(tool_servers_of(&hub), "t", "Linear");
        says_of(&live, current_run(&live), hubs, name, ServerState::Failed).await;

        assert_eq!(worker.told(), ["set", "set"], "{case}");
    }
}

/// So one the server works for stays connected, rather than being put to connecting again, whether it's signed in to again
/// for more access or for nothing it wanted.
#[tokio::test(start_paused = true)]
async fn signing_in_again_is_asked_of_an_agent_that_isnt_connected_and_never_of_one_that_is() {
    let cases = [
        (ServerState::Connected, false, &["set", "set"][..]),
        (ServerState::Connected, true, &["set", "set"]),
        (ServerState::Failed, true, &["set", "set", "Linear"]),
    ];
    for (state, wants_more, told) in cases {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let servers = tool_servers_of(&hub);
        signed_in(servers, "t", "Linear");
        let worker = Worker::reconnecting(&hub).await;
        let live = hub.live_session(&"s".into()).await.unwrap();
        live.ensure_agent().await.unwrap();
        says(&live, current_run(&live), state).await;
        if wants_more {
            let wants = ToolScopes::of(["made-up:write"]);
            let asked = AskForToolAccess { server: "t".into(), refused: TokenNumber::FIRST, wants };
            assert_eq!(servers.wants_access(&asked).unwrap().access, ToolAccess::SignInAgain);
        }
        settled(&live).await;
        assert_eq!(worker.told(), ["set"], "its sign-in is the one the agent launched with");

        again(servers, "t").unwrap();
        settled(&live).await;

        assert_eq!(worker.told(), told, "{state:?}, wanting more: {wants_more}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_sign_in_that_finishes_as_the_agent_starts_is_asked_for_once_the_agent_says_it_failed() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let worker = Worker::reconnecting(&hub).await;
    let live = hub.live_session(&"s".into()).await.unwrap();
    let (start, held) = oneshot::channel();
    *worker.next_start.lock() = Some(held);
    let launching = tokio::spawn({
        let live = Arc::clone(&live);
        async move { live.ensure_agent().await }
    });
    settled(&live).await;

    signed_in(tool_servers_of(&hub), "t", "Linear");
    settled(&live).await;
    start.send(()).unwrap();
    launching.await.unwrap().unwrap();
    settled(&live).await;
    assert_eq!(worker.told(), ["set", "set"], "it hasn't said how it is");
    says(&live, current_run(&live), ServerState::Failed).await;

    assert_eq!(worker.told(), ["set", "set", "Linear"]);
}

/// Nobody asked for it in the chat, so nothing there says it wasn't done, and the next sign-in is asked for as the
/// first was.
#[tokio::test(start_paused = true)]
async fn an_agent_that_refuses_or_has_gone_leaves_nothing_in_the_chat() {
    type Answers = fn() -> Answered;
    let refuses: Answers = || Err(ApiError::new(ErrorCode::Unsupported, "no"));
    let has_gone: Answers = || Ok(WorkerResponse::AgentNotRunning);
    for answers in [refuses, has_gone] {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let servers = tool_servers_of(&hub);
        let worker = Worker::serving(&hub, claims_reconnecting(), answers).await;
        let live = hub.live_session(&"s".into()).await.unwrap();
        live.ensure_agent().await.unwrap();
        says(&live, current_run(&live), ServerState::Failed).await;
        let before = log(&hub).await;

        signed_in(servers, "t", "Linear");
        settled(&live).await;
        assert_eq!(worker.told(), ["set", "set", "Linear"]);
        again(servers, "t").unwrap();
        settled(&live).await;

        assert_eq!(worker.told(), ["set", "set", "Linear", "set", "Linear"]);
        assert_eq!(log(&hub).await, before);
    }
}

/// The hub that starts again doesn't know what the one before it asked, so it asks once more.
#[tokio::test(start_paused = true)]
async fn a_run_carried_on_with_is_asked_for_a_server_that_failed_and_is_signed_in_to() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    signed_in(tool_servers_of(&hub), "t", "Linear");
    let worker = Worker::reconnecting(&hub).await;
    let run = RunId::new(4);
    let claims = claims_reconnecting();
    let kept = protocol::model::RunCapabilities { run, claims: Box::new(claims) };
    hub.blocking(move |store| store.set_claims(&"s".into(), &kept)).await.unwrap();

    let live = carried_on(&hub, &worker.link, run);
    settled(&live).await;
    assert_eq!(worker.told(), ["set"], "nothing says how it is");
    says(&live, run, ServerState::Failed).await;
    assert_eq!(worker.told(), ["set", "Linear"]);
    let live = started_again(&hub, &worker.link, run);
    settled(&live).await;

    assert_eq!(worker.told(), ["set", "Linear", "Linear"]);
}
