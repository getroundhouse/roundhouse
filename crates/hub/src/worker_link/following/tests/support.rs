use std::time::Duration;

use tokio::sync::watch;

/// Once a stand-in's sets are as `done` says, which comes before the clock moves, since nothing else is waited for, and
/// otherwise fails, once the clock has moved a second.
pub(super) async fn until<T: Clone>(sets: &mut watch::Receiver<Vec<T>>, done: impl FnMut(&Vec<T>) -> bool) -> Vec<T> {
    let sets = tokio::time::timeout(Duration::from_secs(1), sets.wait_for(done)).await;
    sets.expect("the stand-in wasn't set what was waited for").unwrap().clone()
}

/// Once a stand-in has been set `count` times, as [`until`] says.
pub(super) async fn until_set<T: Clone>(sets: &mut watch::Receiver<Vec<T>>, count: usize) -> Vec<T> {
    until(sets, |sets| sets.len() >= count).await
}
