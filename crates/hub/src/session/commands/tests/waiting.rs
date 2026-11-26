//! That clients are told what an agent is to connect to once its turn ends, for as long as it's to.

use protocol::capability::setup::{ConnectsAgain, ServerState};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::AgentRunState;
use protocol::model::Observation;
use protocol::worker::WorkerResponse;
use tokio::sync::oneshot;

use super::edited::{ELSEWHERE, QUERIED, URL, at, connected, given, keyed, moved};
use super::reconnecting::{Answered, claims_reconnecting, settled};
use super::reloading::{Running, claims_reloading};
use crate::hub::SessionToolServers;

impl Running {
    /// Whether clients are told it's to connect to the server once its turn ends.
    pub(super) fn waits(&self) -> bool {
        self.connects_again(ConnectsAgain::WhenItsTurnEnds)
    }

    /// Whether clients are told it's cut off from the server until its next start.
    fn cut_off(&self) -> bool {
        self.connects_again(ConnectsAgain::AtItsNextStart)
    }

    fn connects_again(&self, when: ConnectsAgain) -> bool {
        let said = self.hub.observation_value::<SessionToolServers>(&self.live.observed_as()).unwrap();
        said.iter().any(|said| said.connects_again == Some(when))
    }
}

/// As it says how it stands in the turn too, and not where it isn't to be asked. One it's cut off from stays so after the
/// turn.
#[tokio::test(start_paused = true)]
async fn clients_are_told_what_waits_for_the_turn_to_end_until_its_asked_for() {
    for (claims, state, asked, waits, cut_off) in [
        (claims_reloading(), ServerState::Failed, keyed(URL), true, false),
        (claims_reconnecting(), ServerState::Failed, keyed(URL), true, false),
        (claims_reloading(), ServerState::Connected, moved(), true, false),
        (claims_reconnecting(), ServerState::Connected, moved(), false, true),
        (claims_reloading(), ServerState::Connected, keyed(URL), false, false),
    ] {
        let running = given(claims, state).await;
        assert!(!running.waits());
        running.turn(AgentRunState::Running).await;
        assert!(!running.waits());

        running.edited(asked.clone()).await;
        assert_eq!((running.waits(), running.cut_off()), (waits, cut_off), "{state:?}, {asked:?}");
        running.said(state, URL).await;
        assert_eq!((running.waits(), running.cut_off()), (waits, cut_off), "{state:?}, {asked:?}");
        running.turn(AgentRunState::Idle).await;

        assert_eq!((running.waits(), running.cut_off()), (false, cut_off), "{state:?}, {asked:?}");
    }
}

/// So its row doesn't flicker, and a reload that's been asked for isn't taken as answered.
#[tokio::test(start_paused = true)]
async fn a_run_that_says_the_same_again_while_a_server_waits_tells_clients_nothing() {
    let running = given(claims_reloading(), ServerState::Failed).await;
    running.turn(AgentRunState::Running).await;
    running.edited(keyed(URL)).await;
    let mut feed = running.hub.subscribe_feed();

    running.said(ServerState::Failed, URL).await;

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
    reloaded.connects(URL).await;
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

/// It counts only once it reaches the agent, as a person's Reload does, so Claude isn't left connected where the server
/// was. Here the worker says the agent is on a turn, as where its turn started as it was asked.
#[tokio::test(start_paused = true)]
async fn a_moved_server_whose_ask_doesnt_reach_the_agent_is_asked_for_again_the_next_time() {
    let running = connected().await;
    let (answer, held) = oneshot::channel::<Answered>();
    *running.worker.next_control.lock() = Some(held);
    answer.send(Ok(WorkerResponse::AfterTheTurn)).unwrap();

    running.edited(moved()).await;
    assert_eq!(running.told(), ["reload Linear"]);
    assert!(!running.cut_off(), "it can be asked again");
    running.turn(AgentRunState::Running).await;
    assert_eq!(running.told(), ["reload Linear"]);
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["reload Linear", "reload Linear"]);
    running.connects(ELSEWHERE).await;
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
    running.said(ServerState::Failed, URL).await;
    running.turn(AgentRunState::Running).await;
    running.turn(AgentRunState::Idle).await;

    assert_eq!(running.told(), ["Linear"]);
}
