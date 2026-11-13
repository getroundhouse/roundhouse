//! The feed announcing a chat as its wait starts or ends, since its record shows the wait it has open.

use tokio::sync::broadcast::Receiver;

use super::*;
use crate::store::StoreError;

/// The wait each copy of chat `s` the feed has announced since this was last asked shows, by its ID.
fn announced(feed: &mut Receiver<HubNotice>) -> Vec<Option<WaitId>> {
    let mut copies = Vec::new();
    while let Ok(notice) = feed.try_recv() {
        if let HubNotice::Session(session) = notice
            && session.id.as_str() == "s"
        {
            copies.push(session.wait.map(|wait| wait.id));
        }
    }
    copies
}

/// However it starts or ends: as the agent asks, or cancels it, or a read ends it, or its workspace is archived.
#[tokio::test(start_paused = true)]
async fn the_feed_announces_the_chat_with_its_open_wait_as_one_starts_and_with_none_as_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let says = says(Some(open()));
    let link = reading(&hub, &says);
    let mut feed = hub.subscribe_feed();
    // Told of once already, so each copy after is sent only if the record moved.
    hub.notify(HubNotice::Session(hub.store.get_session(&"s".into()).unwrap().session));
    announced(&mut feed);

    let (first, _) = ask(&hub, &link, start("toolu_1")).await.unwrap();
    let started = announced(&mut feed);
    ask(&hub, &link, WorkerAsk::CancelWait { wait: first.id.clone() }).await.unwrap();
    let cancelled = announced(&mut feed);
    let (second, _) = ask(&hub, &link, start("toolu_2")).await.unwrap();
    announced(&mut feed);
    *says.lock() = Ok(Some(PullRequest { state: PrState::Merged, ..open() }));
    hub.workspace_pull_request("w".into()).await.unwrap();
    let merged = announced(&mut feed);
    *says.lock() = Ok(Some(open()));
    ask(&hub, &link, start("toolu_3")).await.unwrap();
    announced(&mut feed);
    hub.call_off_waits_of(&"w".into()).await.unwrap();
    let archived = announced(&mut feed);

    assert_eq!(started.last(), Some(&Some(first.id)));
    assert_eq!(how(&second), None, "it started open, for the read to end");
    let ended: Vec<_> = [cancelled, merged, archived].iter().map(|copies| copies.last().cloned()).collect();
    assert_eq!(ended, [Some(None), Some(None), Some(None)]);
}

/// An agent still running as the archive script runs can ask for a wait, which is refused, since the workspace's record
/// says it's being archived, so archiving leaves none that ends with nobody told.
#[tokio::test(start_paused = true)]
async fn a_wait_asked_for_as_the_archive_script_runs_is_refused_and_none_is_left_open() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let refused = Arc::new(parking_lot::Mutex::new(None));
    let link = WorkerLink::answering({
        let (hub, refused) = (Arc::downgrade(&hub), refused.clone());
        move |request| {
            if matches!(request, WorkerRequest::ArchiveScript(_))
                && let Some(hub) = hub.upgrade()
            {
                let started = hub.store.start_wait(&wait(WaitUntil::Done), Seen::default());
                *refused.lock() = Some(matches!(started, Err(StoreError::NotFound { .. })));
            }
            Some(Ok(WorkerResponse::Ok))
        }
    });
    hub.workers.lock().insert("w".into(), link);
    let mut feed = hub.subscribe_feed();

    hub.workspace_archive("w".into(), true).await.unwrap();

    assert_eq!(*refused.lock(), Some(true), "asked for, and refused");
    assert!(matches!(hub.store.wait(&"a".into()), Err(StoreError::NotFound { .. })), "it never started");
    assert!(announced(&mut feed).iter().all(Option::is_none), "no copy of the chat has one open");
    assert!(hub.store.workspaces_waited_on().unwrap().is_empty());
}
