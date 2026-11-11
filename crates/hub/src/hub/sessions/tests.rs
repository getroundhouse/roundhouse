//! Letting go of a chat that's been deleted, against a hub that never starts a worker, since a chat nothing was sent
//! in never has one.

use std::path::Path;
use std::time::{Duration, Instant};

use providers::local::{LocalConfig, LocalProvider};
use tokio::sync::broadcast::error::RecvError;

use super::*;
use crate::store::tests::store_with_session;

const TIMEOUT: Duration = Duration::from_secs(10);

/// A hub whose store has the session `s`, with nothing sent in it.
fn hub_in(dir: &Path) -> Arc<Hub> {
    let store = Arc::new(store_with_session(dir));
    let config = LocalConfig {
        worker_exe: dir.join("no-worker"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        shell: Some("/bin/sh".to_string()),
        path: None,
    };
    Arc::new(Hub::new(store, dir.join("attachments"), Arc::new(LocalProvider::new(config)), false))
}

#[tokio::test]
async fn closing_an_unused_chat_lets_go_of_it_and_of_everyone_following_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = hub.live_session(&"s".into()).await.unwrap();
    let held = Arc::downgrade(&live);
    // Holding the chat while it waits, as the server does for each client following it.
    let mut following = live.subscribe();
    let follower = tokio::spawn(async move {
        let _holding = live;
        following.recv().await
    });

    hub.session_close("s".into()).await.unwrap();

    let heard = tokio::time::timeout(TIMEOUT, follower).await.expect("the follower was never let go").unwrap();
    assert!(matches!(heard, Err(RecvError::Closed)), "{heard:?}");
    let deadline = Instant::now() + TIMEOUT;
    while held.upgrade().is_some() {
        assert!(Instant::now() < deadline, "something still holds the chat");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(hub.sessions.lock().live.is_empty());
}

#[test]
fn a_session_read_before_it_was_deleted_isnt_kept_once_it_has_been() {
    let mut sessions = Sessions::default();

    assert!(sessions.forget(&"s".into()).is_none(), "nothing of it was in memory");

    let kept = sessions.keep(&"s".into(), || panic!("a deleted session was started"));
    assert!(matches!(kept, Err(e) if e.code == ErrorCode::NotFound));
    assert!(sessions.live.is_empty());
}
