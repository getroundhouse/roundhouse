//! What follows the workspaces a client keeps handles on, driven directly, against a stand-in hub on a Unix socket, for
//! what depends on what a thread has already taken off its channel.
#![cfg(unix)]

use parking_lot::Mutex;
use protocol::client::listed::{Listed, WorkspaceObservations};
use protocol::client::subscriptions::Subscribed;
use protocol::client::{ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::model::{Rev, WorkspaceId};

use super::followed::Wanted;
use super::source::WorkspacesSource;
use crate::client::follow::{Following, Source as _, Unstarted};
use crate::client::{Client, SubscriptionItem};
use crate::records::HubRecords;
use crate::stand_in_hub::{self, Does, refusing_every_start, starts};

/// A client of a stand-in hub on `socket` that answers each request as `answers` says, with the source of what follows
/// its workspaces, the copy of the hub's records it writes into, which no thread follows, and the requests heard.
fn source_of(
    socket: &std::path::Path,
    answers: impl Fn(&ClientRequest) -> ReplyOutcome + Send + 'static,
) -> (Client, WorkspacesSource, Unstarted<HubRecords>, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, move |request| Does::Answers(Box::new(answers(&request))));
    let client = Client::connect(socket, "test").unwrap();
    let records = Following::unstarted(HubRecords::default());
    let source = WorkspacesSource::new(records.reach());
    (client, source, records, heard)
}

/// A start the hub refused isn't asked for again each time another handle comes or goes on a workspace already
/// followed, only once the workspaces followed change.
#[test]
fn a_refused_workspace_is_asked_for_again_only_once_the_workspaces_followed_change() {
    let dir = tempfile::tempdir().unwrap();
    let refusing = |request: &ClientRequest| refusing_every_start(request, ErrorCode::NotFound, "no such workspace");
    let (client, mut source, _, heard) = source_of(&dir.path().join("hub.sock"), refusing);
    let wanted = Mutex::new(Wanted::default());
    wanted.lock().want("w".into());
    let _subscription = source.resume(&client, &wanted).unwrap();

    for _ in 0..3 {
        wanted.lock().want("w".into());
        source.asked(Some(&client), &wanted, &|| {}).unwrap();
    }
    let while_the_same = starts(&heard);
    wanted.lock().want("v".into());
    source.asked(Some(&client), &wanted, &|| {}).unwrap();

    assert_eq!(while_the_same, [1], "asked for once, and refused");
    assert_eq!(starts(&heard), [2], "both, once the workspaces followed changed");
}

/// What a workspace's level sends goes into the copy of the hub's records, whose readers are told, but for what was
/// already on its way as the last handle on it went, such as its list after falling behind, which is dropped, so
/// nothing brings back what letting go of it let go of.
#[test]
fn what_s_sent_of_a_workspace_no_longer_followed_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let started = |_: &ClientRequest| ReplyOutcome::Response(ClientResponse::Subscribed(Subscribed::default()));
    let (client, mut source, records, _) = source_of(&dir.path().join("hub.sock"), started);
    let (wanted, w) = (Mutex::new(Wanted::default()), WorkspaceId::from("w"));
    wanted.lock().want(w.clone());
    let _subscription = source.resume(&client, &wanted).unwrap();
    let listed = || {
        let listed = WorkspaceObservations { workspace_id: w.clone(), observations: Vec::new(), rev: Rev::new(1) };
        SubscriptionItem::Listed(Box::new(Listed::Workspace(listed)))
    };
    let w_listed = || records.reach().read(|records| records.snapshot().workspace_changes(&w).is_some()).unwrap();
    while records.changed().try_recv().is_ok() {}
    source.apply(&client, &wanted, listed()).unwrap();
    assert!(w_listed(), "followed");
    assert!(records.changed().try_recv().is_ok(), "the records' readers are told");

    wanted.lock().unwant(&w);
    source.asked(Some(&client), &wanted, &|| {}).unwrap();
    source.apply(&client, &wanted, listed()).unwrap();

    assert!(!w_listed(), "let go of, and not listed again by what came after");
}
