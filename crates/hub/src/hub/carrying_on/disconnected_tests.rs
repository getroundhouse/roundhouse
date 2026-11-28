//! A workspace whose worker the hub can't reach: how often its provider is asked whether the worker is gone, what a
//! request for a worker does until it's known, and the moment a connection drops.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::ErrorCode;
use providers::WorkerLiveness;
use tokio::time::Instant;

use super::disconnected::Cause;
use crate::hub::Hub;
use crate::hub::stand_in_workers::{a_chat, hub_with_stand_in_workers};
use crate::worker_link::dial_unless_gone;

/// Looks for the worker of `instance` as the wait for it does, which looks again each `within` for as long as it takes,
/// for `for_how_long`.
async fn looking_for(hub: &Arc<Hub>, workspace: &protocol::model::Workspace, for_how_long: Duration) {
    let provider = hub.provider(&workspace.provider).unwrap();
    let (within, until) = (Duration::from_secs(5), Instant::now() + for_how_long);
    while Instant::now() < until {
        let dialed = dial_unless_gone(hub, provider.as_ref(), &workspace.instance_id, within).await;
        assert!(matches!(dialed, Ok(None)), "nothing dialed, and nothing said it was gone");
    }
}

/// So a provider isn't asked every look, which is every five seconds however long the worker has been gone, but less
/// and less often, and at most every half a minute: an hour is about 150 asks, where one at every look would be 720.
#[tokio::test(start_paused = true)]
async fn a_provider_is_asked_less_often_the_longer_a_worker_has_been_disconnected_however_often_the_wait_looks() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, false);
    asked.say_workers_are(WorkerLiveness::Running);

    looking_for(&hub, &workspace, Duration::from_hours(1)).await;

    let asks = asked.state_asks();
    assert!((100..=200).contains(&asks), "{asks} asks in an hour");
}

/// So an adapter that said it can't tell isn't asked again at the next look, or any after it.
#[tokio::test(start_paused = true)]
async fn a_provider_that_cant_say_is_asked_once_however_often_the_wait_looks() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, false);

    looking_for(&hub, &workspace, Duration::from_mins(10)).await;

    assert_eq!(asked.state_asks(), 1);
}

/// A request for a worker mustn't fail only because the provider hasn't been asked yet, so a worker that's gone, which
/// its provider says as soon as it's asked, is replaced as it was when a lost connection meant a lost worker.
#[tokio::test(start_paused = true)]
async fn a_request_for_a_worker_waits_for_its_provider_to_say_whether_it_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    hub.workers.lock().remove(&workspace.id);
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, false);
    let asking = tokio::spawn({
        let (hub, id) = (hub.clone(), workspace.id.clone());
        async move { hub.ensure_worker(&id).await.map(drop) }
    });

    waiting::task::for_what_mustnt_happen(Duration::from_secs(2)).await;
    assert!(!asking.is_finished(), "refused before its provider had been asked");
    // Found gone, and recovered.
    hub.disconnected.clear(&workspace.id);
    let started = asking.await.unwrap();

    // It tried to start one, which a stand-in has no command for, rather than being told it's waited for.
    let error = started.unwrap_err();
    assert!(!error.message.contains("waited for"), "refused: {}", error.message);
}

/// And a worker its provider says nothing of, or that it says may be running, is waited for, as is one it never answers
/// about.
#[tokio::test(start_paused = true)]
async fn a_request_for_a_worker_that_isnt_said_to_be_gone_is_refused_once_that_is_known_or_isnt_in_time() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    hub.workers.lock().remove(&workspace.id);
    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, false);
    let ask = || {
        let (hub, id) = (hub.clone(), workspace.id.clone());
        tokio::spawn(async move { hub.ensure_worker(&id).await.map(drop) })
    };

    let said = ask();
    waiting::task::for_what_mustnt_happen(Duration::from_secs(1)).await;
    hub.disconnected.asked(&workspace.instance_id, crate::hub::Asked::Said);
    let refused = said.await.unwrap().unwrap_err().code;
    // A new workspace's, never asked, is given up on after what a starting hub gives a provider.
    let unasked = ask().await.unwrap().unwrap_err().code;

    assert_eq!((refused, unasked), (ErrorCode::Unavailable, ErrorCode::Unavailable));
}

/// So a request that was waiting for its turn as the connection dropped isn't let in after it: the worker that had it
/// is still there.
#[tokio::test(start_paused = true)]
async fn a_request_waiting_for_its_turn_as_the_connection_drops_is_refused_when_it_gets_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    hub.workers.lock().remove(&workspace.id);
    let gate = hub.worker_gate(&workspace.id);
    let held = gate.lock().await;
    let asking = tokio::spawn({
        let (hub, id) = (hub.clone(), workspace.id.clone());
        async move { hub.ensure_worker(&id).await.map(drop) }
    });
    waiting::task::for_what_mustnt_happen(Duration::from_secs(1)).await;

    hub.disconnected.mark(&workspace.id, &workspace.instance_id, Cause::ConnectionDropped, true);
    drop(held);

    let refused = asking.await.unwrap().unwrap_err();
    assert!(refused.message.contains("waited for"), "it tried to start a worker: {}", refused.message);
}

/// So the workspace is marked as the link is seen to end, while the link still looks open, and one that's been replaced
/// is no concern of it.
#[tokio::test]
async fn the_workspace_is_marked_disconnected_while_the_link_still_looks_open() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    let link = hub.workers.lock().get(&workspace.id).cloned().unwrap();
    let replaced = crate::worker_link::WorkerLink::unconnected();

    let ending = hub.begin_disconnect(&workspace.id, &link, &workspace.instance_id);
    let again = hub.begin_disconnect(&workspace.id, &link, &workspace.instance_id);
    let other = hub.begin_disconnect(&workspace.id, &replaced, &workspace.instance_id);

    assert!(link.is_open(), "nothing has failed its calls yet");
    assert!(hub.disconnected.contains(&workspace.id));
    assert_eq!((ending, again, other), (Some(true), Some(false), None));
}
