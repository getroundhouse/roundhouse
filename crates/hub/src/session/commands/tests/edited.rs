//! Having a chat's agent connect again to a tool server it was given, once the person has edited it: when, and only once.
//! What decides it is tested in `crate::session::had_servers`. Each on tokio's clock, paused, so that nothing more is asked
//! is known once the clock moves.

use std::sync::Arc;

use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::client::tool_servers::ToolServerEdit;
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::model::{RunId, Seq, ToolServerAuthEdit, ToolServerKey, ToolServerUrl};
use protocol::prompt::Prompt;
use protocol::worker::WorkerResponse;
use tokio::sync::oneshot;

use super::launching::{current_run, exited};
use super::reconnecting::{Answered, Worker, says_server, settled};
use super::reloading::{Running, claims_reloading};
use crate::hub::Hub;
use crate::hub::tests::{carried_on, hub_with_worktree, started_again, tool_servers_of};
use crate::session::queue::tests::ended;
use crate::session::{Item, LiveSession};
use crate::tool_servers::tests::{again, edit, edited_by_signing_in, renewed, signed_in, with_key};

pub(super) const URL: &str = "https://mcp.example.com/mcp";
pub(super) const QUERIED: &str = "https://mcp.example.com/mcp?tools=some";
pub(super) const ELSEWHERE: &str = "https://eu.mcp.example.com/mcp";

pub(super) fn at(url: &str) -> ToolServerEdit {
    ToolServerEdit { url: Some(url.into()), ..edit("t") }
}

pub(super) fn keyed(url: &str) -> ToolServerEdit {
    let key = Some(ToolServerKey::new("another"));
    let auth = ToolServerAuthEdit::Key { header: "Authorization".into(), key, url: url.into() };
    ToolServerEdit { auth: Some(auth), ..edit("t") }
}

pub(super) fn moved() -> ToolServerEdit {
    ToolServerEdit { url: Some(ELSEWHERE.into()), ..keyed(ELSEWHERE) }
}

fn edited(hub: &Arc<Hub>, asked: ToolServerEdit) {
    tool_servers_of(hub).edit(asked).unwrap();
}

/// What a run says of the server, which its worker says it last connected to at `url`.
async fn says(live: &LiveSession, run: RunId, state: ServerState, url: &str) {
    let connected_to = Some(ToolServerUrl::typed(url).unwrap().place());
    let (name, hubs) = ("Linear".into(), Some("t".into()));
    let said = ToolServerState { name, hubs, connected_to, state, error: None, tools: None, connects_again: None };
    says_server(live, run, said).await;
}

async fn event(live: &LiveSession, run: RunId, body: EventBody) {
    let body = Box::new(body);
    live.send_item(Item::Event { run, body, raw: None, attachments: Vec::new(), seq: None }).await;
    settled(live).await;
}

/// The server has been added with a key, as the run's agent launched, which is connected to it.
pub(super) async fn connected() -> Running {
    given(claims_reloading(), ServerState::Connected).await
}

pub(super) async fn given(claims: protocol::capability::Capabilities, state: ServerState) -> Running {
    let running = Running::claiming_with(
        claims,
        || Ok(WorkerResponse::Ok),
        |servers| {
            servers.add(with_key("t", "Linear", "made-up")).unwrap();
        },
    );
    let running = running.await;
    running.said(state, URL).await;
    running
}

impl Running {
    pub(super) async fn said(&self, state: ServerState, url: &str) {
        says(&self.live, self.run, state, url).await;
    }

    /// It has connected, having been asked to.
    pub(super) async fn connects(&self, url: &str) {
        self.said(ServerState::Starting, url).await;
        self.said(ServerState::Connected, url).await;
    }

    pub(super) async fn turn(&self, state: AgentRunState) {
        event(&self.live, self.run, EventBody::AgentState { state }).await;
    }

    /// Its agent has taken the one prompt it was given.
    async fn taken(&self) {
        let log = self.hub.blocking(|store| store.read_events(&"s".into(), Seq::FIRST, 100, u64::MAX, false)).await;
        let is_prompt = |body: &EventBody| matches!(body, EventBody::UserPrompt { .. });
        let prompt = log.unwrap().into_iter().find(|event| is_prompt(&event.body)).unwrap().seq;
        event(&self.live, self.run, EventBody::PromptTaken { prompt }).await;
    }

    pub(super) async fn edited(&self, asked: ToolServerEdit) {
        edited(&self.hub, asked);
        settled(&self.live).await;
    }
}

/// One change, which asks a connected Claude: whether each change asks a run, by how it stands and what it claims, is
/// `crate::session::had_servers`' table, and what ends a turn is the session's progress's.
#[tokio::test(start_paused = true)]
async fn a_run_is_asked_once_its_turn_ends_or_at_once_where_its_on_none_and_only_once() {
    let idle = connected().await;
    idle.edited(at(QUERIED)).await;
    assert_eq!(idle.told(), ["reload Linear"]);

    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.edited(at(QUERIED)).await;
    assert_eq!(running.told(), Vec::<String>::new());

    event(&running.live, running.run, ended(TurnOutcome::Success)).await;

    assert_eq!(running.told(), ["reload Linear"]);
    running.connects(URL).await;
    running.turn(AgentRunState::Running).await;
    running.turn(AgentRunState::Idle).await;
    assert_eq!(running.told(), ["reload Linear"], "once");
}

#[tokio::test(start_paused = true)]
async fn a_run_is_asked_only_once_its_worker_has_the_server_as_its_edited() {
    for (state, asked, told) in
        [(ServerState::Failed, keyed(URL), "Linear"), (ServerState::Connected, moved(), "reload Linear")]
    {
        let running = given(claims_reloading(), state).await;
        let (answer, held) = oneshot::channel::<Answered>();
        *running.worker.next_set.lock() = Some(held);

        running.edited(asked).await;
        running.said(state, URL).await;
        running.worker.mark("has it");
        answer.send(Ok(WorkerResponse::Ok)).unwrap();
        settled(&running.live).await;

        assert_eq!(running.told(), ["has it", told]);
    }
}

/// One server changed twice, so the second is had with the first, and the queued prompt is given once it's been asked.
#[tokio::test(start_paused = true)]
async fn whats_due_as_a_turn_ends_is_asked_for_once_and_before_the_next_prompt() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.edited(at(QUERIED)).await;
    running.edited(at(&format!("{QUERIED}&more"))).await;
    running.live.queue("q".into(), Prompt::new("next"), None).await.unwrap();
    settled(&running.live).await;

    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["reload Linear", "prompt"]);
}

/// It was sent as the hub was still giving the agent the one before it, so it was asked for ahead of the turn's end.
#[tokio::test(start_paused = true)]
async fn whats_due_as_a_turn_ends_comes_before_a_prompt_that_was_sent_before_then() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.edited(at(QUERIED)).await;
    let prompt = |text: &str| {
        let (hub, prompt) = (Arc::clone(&running.hub), Prompt::new(text));
        tokio::spawn(async move { hub.session_prompt("s".into(), None, prompt, None).await })
    };
    let (answer, held) = oneshot::channel::<Answered>();
    *running.worker.next_control.lock() = Some(held);
    let first = prompt("first");
    settled(&running.live).await;
    let second = prompt("second");
    settled(&running.live).await;

    running.taken().await;
    event(&running.live, running.run, ended(TurnOutcome::Success)).await;
    assert_eq!(running.told(), ["prompt"]);
    answer.send(Ok(WorkerResponse::Ok)).unwrap();
    settled(&running.live).await;
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();

    assert_eq!(running.told(), ["prompt", "reload Linear", "prompt"]);
}

/// It was given the prompt as it wrote its last reply, and takes it as a turn of its own, which is the one to wait for.
#[tokio::test(start_paused = true)]
async fn a_turn_the_agent_is_about_to_start_with_a_prompt_it_has_been_given_is_one_its_on() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.hub.session_prompt("s".into(), None, Prompt::new("and this"), None).await.unwrap();
    running.edited(at(QUERIED)).await;

    event(&running.live, running.run, ended(TurnOutcome::Success)).await;
    assert_eq!(running.told(), ["prompt"]);
    running.taken().await;
    assert_eq!(running.told(), ["prompt"]);
    event(&running.live, running.run, ended(TurnOutcome::Success)).await;

    assert_eq!(running.told(), ["prompt", "reload Linear"]);
}

#[tokio::test(start_paused = true)]
async fn a_run_that_ends_with_something_due_leaves_nothing_to_ask_the_next() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.edited(at(QUERIED)).await;

    event(&running.live, running.run, exited()).await;
    running.live.ensure_agent().await.unwrap();
    let next = current_run(&running.live);
    says(&running.live, next, ServerState::Connected, URL).await;

    assert_eq!(running.told(), Vec::<String>::new());
}

/// The same one asked for an edit, whatever the run said of the server as it was, and never for renewing it.
#[tokio::test(start_paused = true)]
async fn a_sign_in_is_asked_for_once_the_turn_ends_too_whether_or_not_it_carries_an_edit_and_a_renewal_never_is() {
    for (state, url, after) in [
        (ServerState::Failed, URL, "Linear"),
        (ServerState::NeedsSignIn, URL, "Linear"),
        (ServerState::Failed, ELSEWHERE, "Linear"),
        (ServerState::Connected, QUERIED, "reload Linear"),
        (ServerState::Connected, ELSEWHERE, "reload Linear"),
    ] {
        let running = Running::claiming_with(
            claims_reloading(),
            || Ok(WorkerResponse::Ok),
            |servers| {
                signed_in(servers, "t", "Linear");
            },
        );
        let running = running.await;
        running.said(state, URL).await;
        running.turn(AgentRunState::Running).await;

        renewed(tool_servers_of(&running.hub));
        settled(&running.live).await;
        assert!(!running.waits());
        if url == URL {
            again(tool_servers_of(&running.hub), "t").unwrap();
        } else {
            edited_by_signing_in(tool_servers_of(&running.hub), "t", url);
        }
        settled(&running.live).await;
        assert!(running.waits(), "{state:?} {url}");
        assert_eq!(running.told(), Vec::<String>::new(), "{state:?} {url}");
        running.turn(AgentRunState::Idle).await;

        assert_eq!(running.told(), [after], "{state:?} {url}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_reload_the_person_presses_is_also_what_an_edit_before_it_is_due() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.edited(at(QUERIED)).await;

    assert_eq!(running.reload("Linear").await, Ok(()));
    running.connects(URL).await;
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["reload Linear"]);
}

/// The first was asked before its worker had the server as it's edited, and the second asks nothing, so neither counts.
#[tokio::test(start_paused = true)]
async fn a_second_press_while_the_first_is_under_way_isnt_what_an_edit_between_them_is_due() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;

    assert_eq!(running.reload("Linear").await, Ok(()));
    running.edited(at(QUERIED)).await;
    assert_eq!(running.reload("Linear").await, Ok(()));
    running.connects(URL).await;
    assert_eq!(running.told(), ["reload Linear"]);
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["reload Linear", "reload Linear"]);
}

/// It says how it is again on the next, which has the server as it is, and was told nothing as it was saved.
#[tokio::test(start_paused = true)]
async fn an_edit_saved_while_the_workers_link_is_down_is_asked_for_once_its_back_and_a_link_thats_back_alone_isnt() {
    for (asked, told) in [(Some(at(QUERIED)), vec!["reload Linear"]), (Some(keyed(URL)), vec![]), (None, vec![])] {
        let running = connected().await;
        running.worker.link.close().await;
        if let Some(asked) = asked {
            edited(&running.hub, asked);
        }
        let worker = Worker::running(&running.hub, claims_reloading(), running.run).await;
        running.live.ensure_agent().await.unwrap();

        running.said(ServerState::Connected, URL).await;

        assert_eq!(worker.told().into_iter().filter(|told| told != "set").collect::<Vec<_>>(), told);
    }
}

/// Only where it connected is known of it, so it's asked for a move, once, and nothing else it may have already.
#[tokio::test(start_paused = true)]
async fn a_run_carried_on_with_is_asked_for_a_server_moved_while_no_hub_ran_and_for_nothing_else_its_connected_to() {
    for (asked, told) in [(moved(), vec!["reload Linear"]), (at(QUERIED), vec![])] {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        tool_servers_of(&hub).add(with_key("t", "Linear", "made-up")).unwrap();
        edited(&hub, asked);
        let run = RunId::new(4);
        let worker = Worker::running(&hub, claims_reloading(), run).await;
        let kept = protocol::model::RunCapabilities { run, claims: Box::new(claims_reloading()) };
        hub.blocking(move |store| store.set_claims(&"s".into(), &kept)).await.unwrap();
        let asked = || worker.told().into_iter().filter(|told| told != "set").collect::<Vec<_>>();

        let live = carried_on(&hub, &worker.link, run);
        says(&live, run, ServerState::Connected, URL).await;
        assert_eq!(asked(), told);
        let connected_to = if told.is_empty() { URL } else { ELSEWHERE };
        says(&live, run, ServerState::Starting, URL).await;
        says(&live, run, ServerState::Connected, connected_to).await;
        let live = started_again(&hub, &worker.link, run);
        settled(&live).await;
        says(&live, run, ServerState::Connected, connected_to).await;

        assert_eq!(asked(), told, "the next hub finds it there");
    }
}
