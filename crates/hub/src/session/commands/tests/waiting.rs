//! That clients are told what an agent is to connect to once its turn ends, for as long as it's to.

use protocol::capability::setup::{ConnectsAgain, ServerState};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::AgentRunState;
use protocol::model::Observation;
use protocol::worker::WorkerResponse;
use tokio::sync::oneshot;

use super::edited::{QUERIED, URL, at, connected, given, keyed, moved};
use super::reconnecting::{Answered, claims_reconnecting, settled};
use super::reloading::{Running, claims_reloading};
use crate::hub::SessionToolServers;

impl Running {
    /// Whether clients are told it's to connect to the server once its turn ends.
    pub(super) fn waits(&self) -> bool {
        let said = self.hub.observation_value::<SessionToolServers>(&self.live.observed_as()).unwrap();
        said.iter().any(|said| said.connects_again == Some(ConnectsAgain::WhenItsTurnEnds))
    }
}

/// As it says how it stands in the turn too. Which servers wait, by what changed, how the run stands and what it claims,
/// is `crate::session::had_servers`' to say, in
/// `what_changed_how_the_run_stands_and_what_it_claims_decide_whether_its_asked`.
#[tokio::test(start_paused = true)]
async fn clients_are_told_what_waits_for_the_turn_to_end_until_its_asked_for() {
    let running = given(claims_reloading(), ServerState::Failed).await;
    running.turn(AgentRunState::Running).await;
    assert!(!running.waits());

    running.edited(keyed(URL)).await;
    assert!(running.waits());
    running.said(ServerState::Failed).await;
    assert!(running.waits(), "as the run says how it stands");
    running.turn(AgentRunState::Idle).await;

    assert!(!running.waits());
}

/// So its row doesn't flicker, and a reload that's been asked for isn't taken as answered.
#[tokio::test(start_paused = true)]
async fn a_run_that_says_the_same_again_while_a_server_waits_tells_clients_nothing() {
    let running = given(claims_reloading(), ServerState::Failed).await;
    running.turn(AgentRunState::Running).await;
    running.edited(keyed(URL)).await;
    let mut feed = running.hub.subscribe_feed();

    running.said(ServerState::Failed).await;

    let told = std::iter::from_fn(|| feed.try_recv().ok()).map(std::sync::Arc::unwrap_or_clone);
    let servers =
        told.filter(|notice| matches!(notice, HubNotice::Observation(Observation::SessionToolServers { .. })));
    assert_eq!(servers.collect::<Vec<_>>(), []);
    assert!(running.waits());
}

#[tokio::test(start_paused = true)]
async fn what_waits_is_dropped_where_its_as_the_run_had_it_again_and_where_the_person_reloads_it() {
    let edited_back = connected().await;
    edited_back.turn(AgentRunState::Running).await;
    edited_back.edited(at(QUERIED)).await;
    assert!(edited_back.waits());
    edited_back.edited(at(URL)).await;
    assert!(!edited_back.waits());

    let reloaded = connected().await;
    reloaded.turn(AgentRunState::Running).await;
    reloaded.edited(at(QUERIED)).await;
    assert_eq!(reloaded.reload("Linear").await, Ok(()));
    reloaded.connects().await;
    assert!(!reloaded.waits());
}

/// It took nothing, so it's still to be asked as the turn ends.
#[tokio::test(start_paused = true)]
async fn what_waits_still_does_where_a_reload_is_refused() {
    let running = connected().await;
    running.turn(AgentRunState::Running).await;
    running.edited(at(QUERIED)).await;
    let (answer, held) = oneshot::channel::<Answered>();
    *running.worker.next_control.lock() = Some(held);
    answer.send(Ok(WorkerResponse::AgentNotRunning)).unwrap();

    assert!(running.reload("Linear").await.is_err());
    settled(&running.live).await;
    assert!(running.waits());
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["reload Linear", "reload Linear"]);
    assert!(!running.waits());
}

/// It counts only once it reaches the agent, as a person's Reload does, so Claude isn't left with the tools of where the
/// server was. Here the worker says the agent is on a turn, as where its turn started as it was asked.
#[tokio::test(start_paused = true)]
async fn a_moved_server_whose_ask_doesnt_reach_the_agent_is_asked_for_again_the_next_time() {
    let running = connected().await;
    let (answer, held) = oneshot::channel::<Answered>();
    *running.worker.next_control.lock() = Some(held);
    answer.send(Ok(WorkerResponse::AfterTheTurn)).unwrap();

    running.edited(moved()).await;
    assert_eq!(running.told(), ["reload Linear"]);
    running.turn(AgentRunState::Running).await;
    assert_eq!(running.told(), ["reload Linear"]);
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["reload Linear", "reload Linear"]);
    running.connects().await;
    running.turn(AgentRunState::Running).await;
    running.turn(AgentRunState::Idle).await;
    assert_eq!(running.told(), ["reload Linear", "reload Linear"], "once it's delivered");
}

/// As Codex refuses one it can't connect to, and says it failed again, which would otherwise ask it again, and again.
#[tokio::test(start_paused = true)]
async fn a_server_whose_ask_the_agent_refuses_isnt_asked_for_again() {
    let running = given(claims_reconnecting(), ServerState::Failed).await;
    let (answer, held) = oneshot::channel::<Answered>();
    *running.worker.next_control.lock() = Some(held);
    answer.send(Err(ApiError::new(ErrorCode::Worker, "Codex refused to connect to it again"))).unwrap();

    running.edited(keyed(URL)).await;
    running.said(ServerState::Failed).await;
    running.turn(AgentRunState::Running).await;
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["Linear"]);
}
