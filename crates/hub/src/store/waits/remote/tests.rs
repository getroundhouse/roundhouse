use crate::store::Seen;
use crate::store::tests::store_with_session;
use protocol::fixtures::pull_request_wait;
use protocol::model::{WaitEnd, WaitEnded};

#[test]
fn observations_move_the_session_and_cancellation_freezes_the_result_across_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let wait = pull_request_wait("a");
    store.start_wait(&wait, Seen::default()).unwrap();
    let before = store.get_session(&"s".into()).unwrap().session.rev;
    store
        .revise_remote_waits(None, |wait| {
            let watch = wait.watch.as_mut().unwrap();
            watch.last_attempt_at_ms = 9;
            watch.read_error = Some("offline".into());
        })
        .unwrap();
    let after = store.get_session(&"s".into()).unwrap().session;
    assert!(after.rev > before);
    assert_eq!(after.open_waits()[0].watch.as_ref().unwrap().read_error.as_deref(), Some("offline"));
    store.end_wait(&wait.id, WaitEnded { at_ms: 10, how: WaitEnd::CalledOffByPerson }).unwrap();
    let frozen = store.wait(&wait.id).unwrap();

    let changed =
        store.revise_remote_waits(None, |wait| wait.watch.as_mut().unwrap().snapshot.observed_at_ms = 11).unwrap();

    assert!(changed.is_empty());
    assert_eq!(store.wait(&wait.id).unwrap(), frozen);
    assert!(store.get_session(&"s".into()).unwrap().session.open_waits().is_empty());
    drop(store);
    let reopened = crate::store::tests::open_in(dir.path());
    assert_eq!(reopened.wait(&wait.id).unwrap(), frozen);
}

#[test]
fn deadlines_cancel_only_due_waits_and_preserve_the_last_observation() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut first = pull_request_wait("a");
    first.watch.as_mut().unwrap().deadline_at_ms = Some(10);
    store.start_wait(&first, Seen::default()).unwrap();
    store.start_wait(&pull_request_wait("b"), Seen::default()).unwrap();

    assert!(store.expire_remote_waits(9).unwrap().is_empty());
    let ended = store.expire_remote_waits(10).unwrap();

    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].ended.unwrap().how, WaitEnd::TimedOut);
    assert_eq!(ended[0].watch, first.watch);
    assert!(store.expire_remote_waits(11).unwrap().is_empty());
    assert_eq!(store.remote_waits().unwrap()[0].id.as_str(), "b");
}

#[test]
fn concurrent_retries_insert_one_wait_and_reuse_it_after_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let barrier = std::sync::Barrier::new(2);
    let waits = std::thread::scope(|scope| {
        let start = |id| {
            let mut wait = pull_request_wait(id);
            wait.call = Some("same".into());
            barrier.wait();
            store.start_wait(&wait, Seen::default()).unwrap()
        };
        let a = scope.spawn(move || start("a"));
        let b = scope.spawn(move || start("b"));
        [a.join().unwrap(), b.join().unwrap()]
    });

    assert_eq!(waits[0], waits[1]);
    assert_eq!(store.remote_waits().unwrap().len(), 1);
    store.end_wait(&waits[0].id, WaitEnded { at_ms: 2, how: WaitEnd::Cancelled }).unwrap();
    let retry = store.start_wait(&waits[1], Seen::default()).unwrap();
    assert_eq!(retry.ended.unwrap().how, WaitEnd::Cancelled);
    assert!(store.remote_waits().unwrap().is_empty());
}
