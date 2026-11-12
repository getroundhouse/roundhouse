//! The feed announcing a chat as its wait starts or ends, since its record shows the wait it has open.

use tokio::sync::broadcast::Receiver;

use super::*;

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

/// An agent still running as the archive script runs can start a wait, which archiving ends once its agents have
/// stopped, and the feed hears, where the workspace being marked archived would end it with nobody told.
#[tokio::test(start_paused = true)]
async fn a_wait_started_as_the_archive_script_runs_is_ended_and_the_feed_hears() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering({
        let hub = Arc::downgrade(&hub);
        move |request| {
            if matches!(request, WorkerRequest::ArchiveScript { .. })
                && let Some(hub) = hub.upgrade()
            {
                hub.store.start_wait(&wait(WaitUntil::Done), Seen::default()).unwrap();
                hub.notify(HubNotice::Session(hub.store.get_session(&"s".into()).unwrap().session));
            }
            Some(Ok(WorkerResponse::Ok))
        }
    });
    hub.workers.lock().insert("w".into(), link);
    let mut feed = hub.subscribe_feed();

    hub.workspace_archive("w".into(), true).await.unwrap();

    let copies = announced(&mut feed);
    assert_eq!(copies.first(), Some(&Some("a".into())), "the wait started");
    assert_eq!(copies.last(), Some(&None), "and the feed heard it end");
    assert_eq!(how(&hub.store.wait(&"a".into()).unwrap()), Some(WaitEnd::Cancelled));
}
