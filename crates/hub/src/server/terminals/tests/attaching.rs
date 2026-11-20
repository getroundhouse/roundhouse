//! Telling a terminal's worker whether any client is attached, so it sends what the program writes only while one is.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::terminal::TerminalUpdate;
use protocol::worker::WorkerResponse;

use super::stand_in::{Asked, Client, WAIT, open_noting, screen};

/// The worker hears that no client is attached once the terminal opens, that one is before it's asked what the
/// terminal shows, and that none is once the last has gone, but not as one of two goes. On several threads, where the
/// worker could be told as the last goes before its subscription has.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_terminals_worker_is_told_as_its_first_client_comes_and_its_last_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = open_noting(dir.path(), |_| Some(Ok(WorkerResponse::Ok))).await;
    waiting::task::until(WAIT, "the worker was never told no client is attached", || !asked.lock().is_empty()).await;

    let (mut first, mut second) = (Client::attach(&hub), Client::attach(&hub));
    assert_eq!(first.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(second.next().await, TerminalUpdate::Screen(screen()));
    first.leave().await;
    second.leave().await;

    waiting::task::until(WAIT, "the worker was never told the last went", || asked.lock().len() == 5).await;
    let told = asked.lock().clone();
    let attached = [Asked::Attached(false), Asked::Attached(true), Asked::Show, Asked::Show, Asked::Attached(false)];
    assert_eq!(told, attached);
}

/// Since a screen it's sent before it's told would be followed by nothing.
#[tokio::test(start_paused = true)]
async fn a_client_is_sent_no_screen_until_the_worker_knows_its_attached() {
    let dir = tempfile::tempdir().unwrap();
    let refused = ApiError::new(ErrorCode::Worker, "not now");
    // Not the first, as it opens, but the next, as the client attaches.
    let (hub, asked) =
        open_noting(dir.path(), move |n| Some(if n == 1 { Err(refused.clone()) } else { Ok(WorkerResponse::Ok) }))
            .await;
    waiting::task::until(WAIT, "the worker was never told no client is attached", || !asked.lock().is_empty()).await;

    let mut client = Client::attach(&hub);

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    let attached = [Asked::Attached(false), Asked::Attached(true), Asked::Attached(true), Asked::Show];
    assert_eq!(*asked.lock(), attached);
}

/// A client that goes while the worker is being told it's attached, which the telling may have reached, so the hub
/// can't take the worker to know none is.
#[tokio::test(start_paused = true)]
async fn a_worker_whose_telling_was_cut_short_is_told_again() {
    let dir = tempfile::tempdir().unwrap();
    // The first, as it opens, is answered, and the next, as the client attaches, never is.
    let (hub, asked) = open_noting(dir.path(), |n| (n != 1).then_some(Ok(WorkerResponse::Ok))).await;
    waiting::task::until(WAIT, "the worker was never told no client is attached", || !asked.lock().is_empty()).await;

    let client = Client::attach(&hub);
    waiting::task::until(WAIT, "the worker was never told a client is", || asked.lock().len() == 2).await;
    client.leave().await;

    waiting::task::until(WAIT, "the worker was never told again", || asked.lock().len() == 3).await;
    assert_eq!(*asked.lock(), [Asked::Attached(false), Asked::Attached(true), Asked::Attached(false)]);
}

/// As one from before it could be told isn't again, which sends everything.
#[tokio::test(start_paused = true)]
async fn a_worker_that_cant_be_told_isnt_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let unsupported = ApiError::new(ErrorCode::Unsupported, "no such request");
    let (hub, asked) = open_noting(dir.path(), move |_| Some(Err(unsupported.clone()))).await;
    waiting::task::until(WAIT, "the worker was never told", || !asked.lock().is_empty()).await;

    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    client.leave().await;

    waiting::task::for_what_mustnt_happen(Duration::from_millis(50)).await;
    assert_eq!(*asked.lock(), [Asked::Attached(false), Asked::Show]);
}
