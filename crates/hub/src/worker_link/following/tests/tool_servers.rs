//! Links following the hub's tool servers. What a link follows is tested where it's the git credential's too.

use std::path::Path;
use std::sync::Arc;

use logging::testing::Logs;
use protocol::client::tool_servers::ToolServerAdd;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServerAuthGiven, ToolServerKey};
use protocol::worker::{ToolServersSet, WorkerRequest, WorkerResponse};
use tokio::sync::watch;

use super::super::{Following, SET_WITHIN, Servers};
use super::support::{until, until_set};
use crate::store::tests::open_in;
use crate::tool_servers::ToolServers;
use crate::worker_link::WorkerLink;

fn servers_in(dir: &Path) -> ToolServers {
    ToolServers::new(Arc::new(open_in(dir)), dir)
}

fn add(servers: &ToolServers, id: &str, name: &str, auth: ToolServerAuthGiven) {
    let url = "https://mcp.example.com/mcp".into();
    servers.add(ToolServerAdd { id: id.into(), name: name.into(), url, auth }).unwrap();
}

fn a_key() -> ToolServerAuthGiven {
    ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new("Bearer made-up") }
}

/// A stand-in for a worker, whose opened link follows `servers`, which keeps each set it's sent, in turn, and answers
/// each as `answer` says, or never, for `None`.
fn following(
    servers: &ToolServers,
    answer: impl Fn() -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
) -> (Arc<WorkerLink>, watch::Receiver<Vec<ToolServersSet>>) {
    let (kept, sets) = watch::channel(Vec::new());
    let link = WorkerLink::answering(move |request| {
        let WorkerRequest::ToolServersSet(set) = request else { panic!("{request:?}") };
        kept.send_modify(|sets| sets.push(set));
        answer()
    });
    tokio::spawn(Following::new(Arc::clone(&link), Servers::new(servers.sent())).follow());
    (link, sets)
}

/// The name agents are given a server by, what the person calls it, and the header each request to it carries.
type Spelled<'a> = (&'a str, &'a str, Option<(&'a str, &'a str)>);

/// Each server of `set`.
fn spelled(set: &ToolServersSet) -> Vec<Spelled<'_>> {
    let spelled = set.servers.iter().map(|server| {
        assert_eq!(server.url.as_str(), "https://mcp.example.com/mcp");
        let carried = server.header.as_ref().map(|header| (header.name.as_str(), header.value.as_str()));
        (server.name.as_str(), server.label.as_str(), carried)
    });
    spelled.collect()
}

#[tokio::test(start_paused = true)]
async fn every_link_is_sent_the_whole_set_as_a_server_is_added_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let servers = servers_in(dir.path());
    let mut links: Vec<_> = (0..2).map(|_| following(&servers, || Some(Ok(WorkerResponse::Ok)))).collect();
    let key = Some(("Authorization", "Bearer made-up"));
    let mut made = Vec::new();
    // Each once the last has reached every link, since a link is sent only the newest.
    let changes: [&dyn Fn(); 3] = [
        &|| add(&servers, "k", "Linear (Northwind)", a_key()),
        &|| add(&servers, "n", "Docs", ToolServerAuthGiven::Neither),
        &|| drop(servers.remove(&"k".into()).unwrap()),
    ];

    for (at, change) in changes.iter().enumerate() {
        change();
        for (_, sets) in &mut links {
            made.push(until_set(sets, at + 1).await);
        }
    }

    for sets in &made[made.len() - 2..] {
        let sets: Vec<_> = sets.iter().map(spelled).collect();
        let linear = ("Linear_Northwind", "Linear (Northwind)", key);
        let expected = [vec![linear], vec![("Docs", "Docs", None), linear], vec![("Docs", "Docs", None)]];
        assert_eq!(sets, expected);
    }
    assert!(links.iter().all(|(link, _)| link.is_open()));
}

/// A worker from before it passed calls on, which answers a request it doesn't know so.
#[tokio::test(start_paused = true)]
async fn an_older_worker_goes_without_and_is_sent_no_more_keys_and_nothing_is_stopped_for_it() {
    let logs = Logs::capture();
    let dir = tempfile::tempdir().unwrap();
    let servers = servers_in(dir.path());
    let older = || Some(Err(ApiError::new(ErrorCode::Unsupported, "this worker doesn't know the request")));
    let (link, mut sets) = following(&servers, older);
    let (current, mut current_sets) = following(&servers, || Some(Ok(WorkerResponse::Ok)));

    add(&servers, "k", "Linear", a_key());
    until_set(&mut sets, 1).await;
    until_set(&mut current_sets, 1).await;
    add(&servers, "n", "Docs", ToolServerAuthGiven::Neither);
    until_set(&mut current_sets, 2).await;
    tokio::time::advance(SET_WITHIN).await;

    assert_eq!(sets.borrow().len(), 1);
    assert!(link.is_open() && current.is_open());
    let told = logs.events().iter().filter(|e| e["event"] == "worker.tool_servers_unsupported").count();
    assert_eq!(told, 1, "{}", logs.text());
    assert!(!logs.text().contains("made-up"), "{}", logs.text());
}

/// So it can't go on with a server that was removed.
#[tokio::test(start_paused = true)]
async fn a_worker_that_fails_a_set_or_never_answers_one_is_ended_and_holds_up_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let servers = servers_in(dir.path());
    let (failing, _failing_sets) =
        following(&servers, || Some(Err(ApiError::new(ErrorCode::Internal, "made-up trouble"))));
    let (stuck, mut stuck_sets) = following(&servers, || None);
    let (answering, mut answered) = following(&servers, || Some(Ok(WorkerResponse::Ok)));

    add(&servers, "k", "Linear", a_key());
    until_set(&mut stuck_sets, 1).await;
    servers.remove(&"k".into()).unwrap();
    until(&mut answered, |sets| sets.last().is_some_and(|set| set.servers.is_empty())).await;
    let failed = tokio::time::timeout(SET_WITHIN / 2, failing.wait_closed()).await;
    let stuck_meanwhile = stuck.is_open();
    let ended = tokio::time::timeout(SET_WITHIN * 2, stuck.wait_closed()).await;

    assert!(failed.is_ok(), "the one that refused wasn't ended at once");
    assert!(stuck_meanwhile);
    assert!(ended.is_ok(), "the one that never answered wasn't ended");
    assert!(answering.is_open());
}
