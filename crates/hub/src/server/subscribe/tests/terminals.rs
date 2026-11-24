//! What following a terminal has its worker do.

use protocol::client::subscriptions::{Follow, Subscribe};

use super::given::{Given, ok, start};
use crate::hub::Followed;
use crate::server::tests::{Connection, WAIT};

/// A worker stops sending what its terminal shows once nothing follows it, whether a subscription stopped or its
/// connection ended.
#[tokio::test]
async fn a_worker_stops_sending_a_terminals_screen_once_no_client_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let (mut ending, mut stopping) =
        (Connection::greeted(&given.hub, 4).await, Connection::greeted(&given.hub, 5).await);
    let follows = || Subscribe { stop: Vec::new(), start: vec![start(7, Follow::Terminal { id: "t".into() })] };
    ok(&mut ending, 1, follows()).await;
    ok(&mut stopping, 1, follows()).await;
    waiting::task::until(WAIT, "the worker to be told t is watched", || given.told() == [true]).await;

    ok(&mut stopping, 2, Subscribe { stop: vec![7], start: Vec::new() }).await;
    let told_while_one_follows = given.told();
    drop(ending);

    waiting::task::until(WAIT, "the worker to be told nothing watches t", || given.told() == [true, false]).await;
    assert_eq!(told_while_one_follows, [true]);
    assert!(!given.hub.followed(&Followed::Terminal("t".into())));
}
