use crate::store::tests::store_with_session;
use protocol::fixtures::pull_request_wait;
use protocol::model::{WaitEnd, WaitEnded};
use std::collections::HashMap;

/// An open wait's record keeps none of what it read, which the hub keeps apart: it's judged with what it's given of
/// that, and its record and its chat's revision are left. One that ends keeps what it last read, frozen, so a late read
/// changes nothing of it, across reopening too.
#[test]
fn an_open_waits_record_keeps_none_of_what_it_read_and_one_that_ends_keeps_its_last_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let wait = pull_request_wait("a");
    store.start_wait(&wait).unwrap();
    let recorded = store.wait(&wait.id).unwrap();
    let before = store.get_session(&"s".into()).unwrap().session.rev;
    let read = wait.watch.as_ref().unwrap().read();
    let reads = HashMap::from([(wait.id.clone(), read)]);

    let judged = store
        .revise_remote_waits(None, &reads, |wait| {
            let watch = wait.watch.as_mut().unwrap();
            watch.last_attempt_at_ms = 9;
            watch.read_error = Some("offline".into());
        })
        .unwrap();

    let watch = judged[0].watch.as_ref().unwrap();
    assert_eq!((watch.read_error.as_deref(), watch.snapshot.pull_requests.len()), (Some("offline"), 1));
    assert_eq!(recorded.watch.as_ref().unwrap().snapshot.pull_requests, [], "none of the read");
    assert_eq!(store.wait(&wait.id).unwrap(), recorded, "nothing of the record changed");
    assert_eq!(store.get_session(&"s".into()).unwrap().session.rev, before);
    let ended = store
        .revise_remote_waits(None, &reads, |wait| wait.ended = Some(WaitEnded { at_ms: 10, how: WaitEnd::Merged }))
        .unwrap();
    let frozen = store.wait(&wait.id).unwrap();
    assert_eq!(frozen, ended[0]);
    assert_eq!(frozen.watch, wait.watch, "the last read, frozen");

    let changed = store
        .revise_remote_waits(None, &HashMap::new(), |wait| wait.watch.as_mut().unwrap().snapshot.observed_at_ms = 11)
        .unwrap();

    assert!(changed.is_empty());
    assert_eq!(store.wait(&wait.id).unwrap(), frozen);
    assert!(store.get_session(&"s".into()).unwrap().session.open_waits().is_empty());
    drop(store);
    let reopened = crate::store::tests::open_in(dir.path());
    assert_eq!(reopened.wait(&wait.id).unwrap(), frozen);
}

#[test]
fn deadlines_end_only_due_waits_and_leave_their_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut first = pull_request_wait("a");
    first.watch.as_mut().unwrap().deadline_at_ms = Some(10);
    store.start_wait(&first).unwrap();
    store.start_wait(&pull_request_wait("b")).unwrap();

    assert!(store.expire_remote_waits(9).unwrap().is_empty());
    let ended = store.expire_remote_waits(10).unwrap();

    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].ended.unwrap().how, WaitEnd::TimedOut);
    // What it last read is the hub's, which keeps it in the record once it has ended.
    assert_eq!(ended[0].watch.as_deref(), Some(&first.watch.as_ref().unwrap().recorded()));
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
            store.start_wait(&wait).unwrap()
        };
        let a = scope.spawn(move || start("a"));
        let b = scope.spawn(move || start("b"));
        [a.join().unwrap(), b.join().unwrap()]
    });

    assert_eq!(waits[0], waits[1]);
    assert_eq!(store.remote_waits().unwrap().len(), 1);
    store.end_wait(&waits[0].id, WaitEnded { at_ms: 2, how: WaitEnd::Cancelled }).unwrap();
    let retry = store.start_wait(&waits[1]).unwrap();
    assert_eq!(retry.ended.unwrap().how, WaitEnd::Cancelled);
    assert!(store.remote_waits().unwrap().is_empty());
}
