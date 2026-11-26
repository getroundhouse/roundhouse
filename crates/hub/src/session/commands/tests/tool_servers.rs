//! The tool servers a session's agent has, which are only ever its current run's.

use std::sync::Arc;

use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::capability::{Capabilities, Claimed, Report};
use protocol::client::{HubNotice, ObservationNotice};
use protocol::event::EventBody;
use protocol::model::{Observation, ObservationKey, Observed, RunId};

use super::launching::{current_run, exited, worker};
use crate::hub::Hub;
use crate::hub::tests::{claimed, flushed, hub_with_worktree};
use crate::session::{Item, LiveSession};

fn says_servers() -> Capabilities {
    let mut claims = claimed();
    claims.setup.tool_servers = Some(Claimed {});
    claims
}

/// What `run` says of its servers, once the session has taken it in: that it has the one called `name`.
async fn says(live: &LiveSession, run: RunId, name: &str) {
    let state = ServerState::Connected;
    let server =
        ToolServerState { name: name.into(), hubs: None, connected_to: None, state, error: None, tools: Some(2) };
    let report = Report::ToolServers { servers: vec![server] };
    live.send_item(Item::Report { run, report, seq: None }).await;
    flushed(live).await;
}

async fn exits(live: &LiveSession, run: RunId) {
    let body: Box<EventBody> = Box::new(exited());
    live.send_item(Item::Event { run, body, raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(live).await;
}

/// The names of the servers `hub` lists at `s`'s level, as of when they were said, or `None` where they aren't known.
async fn listed(hub: &Hub) -> Option<Vec<String>> {
    let listed = hub.session_records("s".into()).await.unwrap().observations;
    listed.into_iter().find_map(|observation| {
        let Observation::SessionToolServers { servers, .. } = observation else { return None };
        let Observed::AsOf { value, .. } = servers else { panic!("never current, but {servers:?}") };
        Some(value.into_iter().map(|server| server.name.to_string()).collect())
    })
}

/// Starts the agent, and returns its run.
async fn started(live: &LiveSession) -> RunId {
    live.ensure_agent().await.unwrap();
    current_run(live)
}

/// They're listed and told of as the run says them, and go as it ends, which is told too. The next run has none until
/// it says, and a run that was replaced says nothing more of them, nor does its exit, however late, take the next
/// one's.
#[tokio::test]
async fn a_chats_tool_servers_are_what_its_current_run_says_and_go_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub: Arc<Hub> = hub_with_worktree(dir.path());
    let (_link, _asked) = worker(&hub, None, says_servers());
    let live = hub.live_session(&"s".into()).await.unwrap();
    let mut feed = hub.subscribe_feed();

    let first = started(&live).await;
    says(&live, first, "docs").await;
    assert_eq!(listed(&hub).await, Some(vec!["docs".into()]));
    exits(&live, first).await;
    assert_eq!(listed(&hub).await, None, "not known, rather than none, once it has ended");

    let second = started(&live).await;
    says(&live, second, "tickets").await;
    // The hub heard that it isn't running before it heard that it exited.
    live.state.lock().agent = None;
    let third = started(&live).await;
    assert_eq!(listed(&hub).await, None, "another run has none until it says");
    says(&live, second, "tickets").await;
    assert_eq!(listed(&hub).await, None, "nor does the one before it say any more");
    says(&live, third, "mail").await;
    exits(&live, second).await;
    assert_eq!(listed(&hub).await, Some(vec!["mail".into()]), "its late exit leaves the next run's");

    let key = ObservationKey::SessionToolServers { session_id: "s".into() };
    let mut told = Vec::new();
    while let Ok(notice) = feed.try_recv() {
        match ObservationNotice::try_from(HubNotice::clone(&notice)) {
            Ok(ObservationNotice::Copy(copy)) if copy.key() == key => told.push("says"),
            Ok(ObservationNotice::Removed { observation, .. }) if observation == key => told.push("gone"),
            _ => {}
        }
    }
    assert_eq!(told, ["says", "gone", "says", "gone", "says"]);
}
