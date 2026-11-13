//! A hub's terminals, with a stand-in for `w`'s worker that runs any terminal it's asked to.

use std::sync::{OnceLock, Weak};
use std::time::Duration;

use protocol::client::terminals::TerminalOpen;
use protocol::model::Records;
use protocol::terminal::{TerminalBytes, TerminalExit};
use protocol::worker::{TerminalStart, WorkerRequest, WorkerResponse};
use tokio::time::timeout;

use super::*;
use crate::hub::tests::{as_worker_of_w, hub_with_worktree, terminal_in_w as open};

/// The hub, with a stand-in as `w`'s worker that answers every call, and what it was asked.
fn with_worker(dir: &std::path::Path) -> (Arc<Hub>, Arc<WorkerLink>, Arc<Mutex<Vec<WorkerRequest>>>) {
    let hub = hub_with_worktree(dir);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let keeping = asked.clone();
    let link = WorkerLink::answering(move |request| {
        keeping.lock().push(request);
        Some(Ok(WorkerResponse::Ok))
    });
    as_worker_of_w(&hub, &link);
    (hub, link, asked)
}

/// For a stand-in worker to reach the hub, and its own link, as a worker sends frames as well as replies.
type Reaching = Arc<OnceLock<(Weak<Hub>, Weak<WorkerLink>)>>;

fn reached(reaching: &Reaching) -> (Arc<Hub>, Arc<WorkerLink>) {
    let (hub, link) = reaching.get().expect("the stand-in to know its hub");
    (hub.upgrade().unwrap(), link.upgrade().unwrap())
}

fn exited() -> TerminalUpdate {
    TerminalUpdate::Exited(TerminalExit { code: Some(0) })
}

fn output(text: &str) -> TerminalUpdate {
    TerminalUpdate::Output { output: TerminalBytes::from(text.as_bytes()) }
}

async fn next(feed: &mut broadcast::Receiver<HubNotice>) -> HubNotice {
    timeout(Duration::from_secs(10), feed.recv()).await.expect("a notice").unwrap()
}

async fn listed(hub: &Hub) -> Records {
    hub.records_list().await.unwrap()
}

/// In the worktree, with what its agents get, and only once however often it's asked.
#[tokio::test]
async fn a_terminal_opens_in_the_worktree_with_the_workspaces_environment() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link, asked) = with_worker(dir.path());
    let mut feed = hub.subscribe_feed();

    let opened = hub.terminal_open(open("t")).await.unwrap();
    let again = hub.terminal_open(open("t")).await.unwrap();

    assert_eq!(again, opened);
    let starts: Vec<TerminalStart> = asked
        .lock()
        .iter()
        .filter_map(|asked| if let WorkerRequest::TerminalStart(start) = asked { Some(start.clone()) } else { None })
        .collect();
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert_eq!((starts[0].terminal.as_str(), starts[0].cwd.as_str(), starts[0].size), ("t", "/w", open("t").size));
    assert_eq!(starts[0].opened_at_ms, opened.opened_at_ms, "the worker isn't told when the hub opened it");
    assert!(starts[0].env.iter().any(|(name, value)| name == "CONDUCTOR_WORKSPACE_ID" && value == "w"));
    assert_eq!(next(&mut feed).await, HubNotice::Terminal(opened.clone()));
    let records = listed(&hub).await;
    assert_eq!(records.terminals, vec![opened.clone()]);
    assert_eq!(records.rev, opened.rev, "the terminal's revision isn't from the records' counter");
}

/// Whether the worker refuses it, or the hub does before asking the worker, nothing is left of it.
#[tokio::test]
async fn a_terminal_that_cant_open_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|request| {
        let refused = matches!(request, WorkerRequest::TerminalStart(_));
        Some(if refused { Err(ApiError::new(ErrorCode::Worker, "no PTY")) } else { Ok(WorkerResponse::Ok) })
    });
    as_worker_of_w(&hub, &link);

    let refused = hub.terminal_open(open("t")).await;
    let left = hub.terminals.count();
    let nowhere = hub.terminal_open(TerminalOpen { workspace_id: "nowhere".into(), ..open("u") }).await;

    assert_eq!(refused.map_err(|e| e.code), Err(ErrorCode::Worker));
    assert_eq!(left, 0);
    assert!(nowhere.is_err());
    assert_eq!(hub.terminals.count(), 0);
    assert_eq!(listed(&hub).await.terminals, Vec::new());
}

/// One sent again before the first has opened it waits for that, rather than starting it too, so the two can't
/// undo each other, as one failing took away what the other noted.
#[tokio::test]
async fn opening_a_terminal_again_as_it_opens_waits_for_how_that_ends() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reaching = Reaching::default();
    let (starts, again) = (Arc::new(Mutex::new(0)), Arc::new(Mutex::new(None)));
    let (ends, counting, opening_again) = (reaching.clone(), starts.clone(), again.clone());
    let link = WorkerLink::answering(move |request| {
        if !matches!(request, WorkerRequest::TerminalStart(_)) {
            return Some(Ok(WorkerResponse::Ok));
        }
        *counting.lock() += 1;
        if *counting.lock() > 1 {
            return Some(Err(ApiError::new(ErrorCode::Worker, "no PTY")));
        }
        // Sent as the first is opening it, and taken up before the first hears from the worker.
        let (hub, _) = reached(&ends);
        *opening_again.lock() = Some(tokio::spawn(async move { hub.terminal_open(open("t")).await }));
        Some(Ok(WorkerResponse::Ok))
    });
    reaching.set((Arc::downgrade(&hub), Arc::downgrade(&link))).unwrap();
    as_worker_of_w(&hub, &link);

    let opened = hub.terminal_open(open("t")).await;
    let again = again.lock().take().expect("the second open");
    let again = again.await.unwrap();

    assert_eq!(*starts.lock(), 1);
    let opened = opened.unwrap();
    assert_eq!(again, Ok(opened.clone()));
    assert_eq!(listed(&hub).await.terminals, vec![opened]);
}

/// Its worker refuses it, as a build from before terminals does, until the hub replaces it once nothing is at work in
/// the workspace, so a client shouldn't take terminals for something its hub doesn't have.
#[tokio::test]
async fn a_worker_from_before_terminals_makes_them_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|request| {
        let refused = matches!(request, WorkerRequest::TerminalStart(_));
        Some(if refused {
            Err(ApiError::new(ErrorCode::Unsupported, "unknown request"))
        } else {
            Ok(WorkerResponse::Ok)
        })
    });
    as_worker_of_w(&hub, &link);

    let refused = hub.terminal_open(open("t")).await;

    assert_eq!(refused.map_err(|e| e.code), Err(ErrorCode::Unavailable));
}

/// The shell's end, however it comes, closes the terminal, which a client then drops.
#[tokio::test]
async fn a_terminal_whose_shell_ends_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = with_worker(dir.path());
    let opened = hub.terminal_open(open("t")).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.terminal_update(&link, "t".into(), UpdateIndex::new(12), exited());

    let HubNotice::Removed { record, rev } = next(&mut feed).await else { panic!("the terminal wasn't removed") };
    assert_eq!(record, RecordKey::Terminal { id: "t".into() });
    assert!(rev.supersedes(opened.rev));
    assert_eq!(listed(&hub).await.terminals, Vec::new());
    assert!(!hub.terminals.open_in(&"w".into()));
}

/// What one worker says of a terminal doesn't reach one another worker runs, such as its replacement's.
#[tokio::test]
async fn what_another_worker_says_doesnt_reach_a_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = with_worker(dir.path());
    hub.terminal_open(open("t")).await.unwrap();
    let (_, mut updates) = hub.terminal_follow(&"t".into()).unwrap();
    let other = WorkerLink::unconnected();

    hub.terminal_update(&other, "t".into(), UpdateIndex::new(0), output("theirs"));
    hub.terminal_ended(&"t".into(), other.id).await;
    hub.terminal_update(&link, "t".into(), UpdateIndex::new(0), output("ours"));

    assert_eq!(updates.recv().await.unwrap().update, output("ours"));
    assert_eq!(listed(&hub).await.terminals.len(), 1);
}

/// Whichever reaches the hub first, the open or its shell's end, no record is left of it.
#[tokio::test]
async fn a_shell_that_ends_as_its_terminal_opens_leaves_no_record() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reaching = Reaching::default();
    let ends = reaching.clone();
    let link = WorkerLink::answering(move |request| {
        if matches!(request, WorkerRequest::TerminalStart(_)) {
            let (hub, link) = reached(&ends);
            hub.terminal_update(&link, "t".into(), UpdateIndex::new(0), exited());
        }
        Some(Ok(WorkerResponse::Ok))
    });
    reaching.set((Arc::downgrade(&hub), Arc::downgrade(&link))).unwrap();
    as_worker_of_w(&hub, &link);

    let _ = hub.terminal_open(open("t")).await;

    timeout(Duration::from_secs(10), async {
        while !listed(&hub).await.terminals.is_empty() || hub.terminals.open_in(&"w".into()) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the terminal to go");
}

#[tokio::test]
async fn closing_a_terminal_ends_its_shell_and_removes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link, asked) = with_worker(dir.path());
    let opened = hub.terminal_open(open("t")).await.unwrap();

    let closed = hub.terminal_close(TerminalClose { id: "t".into() }).await.unwrap();

    assert_eq!(closed, opened);
    let stop = WorkerRequest::TerminalStop(worker::TerminalStop { terminal: "t".into() });
    assert!(asked.lock().contains(&stop), "the shell wasn't ended");
    assert_eq!(listed(&hub).await.terminals, Vec::new());
    let again = hub.terminal_close(TerminalClose { id: "t".into() }).await;
    assert_eq!(again.map_err(|e| e.code), Err(ErrorCode::NotFound));
}

/// Its shell went with it.
#[tokio::test]
async fn a_terminal_whose_worker_has_gone_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = with_worker(dir.path());
    hub.terminal_open(open("t")).await.unwrap();
    let workspace = hub.workspace(&"w".into()).await.unwrap();
    let provider = hub.provider(&workspace.provider).unwrap();

    hub.worker_gone(&"w".into(), &link, provider.as_ref(), &workspace.instance_id).await;

    assert_eq!(listed(&hub).await.terminals, Vec::new());
}

/// As a hub that starts carries on with what a worker an earlier hub left runs, each as it was opened.
#[tokio::test]
async fn a_hub_carries_on_with_the_terminals_a_worker_it_finds_runs() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let running = RunningTerminal { terminal: "t".into(), opened_at_ms: 5 };
    let reply = worker::RunningTerminalsReply { terminals: vec![running] };
    let link = WorkerLink::answering(move |_| Some(Ok(WorkerResponse::RunningTerminals(reply.clone()))));
    as_worker_of_w(&hub, &link);

    let carried_on = hub.carry_on_terminals(&"w".into(), &link).await;

    assert_eq!(carried_on, 1);
    let records = listed(&hub).await.terminals;
    assert_eq!(records.iter().map(|record| (record.id.as_str(), record.opened_at_ms)).collect::<Vec<_>>(), [("t", 5)]);
    assert!(hub.open_terminal(&"t".into()).is_ok(), "it isn't the worker's");
}

/// The worker lists a shell, which then ends, and its end can reach the hub before the list does, or just after it.
#[tokio::test]
async fn a_shell_that_ends_as_a_hub_carries_on_with_it_takes_its_record_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reaching = Reaching::default();
    let ends = reaching.clone();
    let link = WorkerLink::answering(move |_| {
        let (hub, link) = reached(&ends);
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(0), exited());
        let running = RunningTerminal { terminal: "t".into(), opened_at_ms: 5 };
        Some(Ok(WorkerResponse::RunningTerminals(worker::RunningTerminalsReply { terminals: vec![running] })))
    });
    reaching.set((Arc::downgrade(&hub), Arc::downgrade(&link))).unwrap();
    as_worker_of_w(&hub, &link);
    let mut feed = hub.subscribe_feed();

    hub.carry_on_terminals(&"w".into(), &link).await;

    assert!(matches!(next(&mut feed).await, HubNotice::Terminal(_)));
    let HubNotice::Removed { record, .. } = next(&mut feed).await else { panic!("the terminal wasn't removed") };
    assert_eq!(record, RecordKey::Terminal { id: "t".into() });
    assert!(!hub.terminals.open_in(&"w".into()));
}

/// Stopping it would stop its workers, and end the shells, as replacing an idle hub of an older build does, so its
/// status counts them, for a client to say what stopping it ends.
#[tokio::test]
async fn a_hub_with_a_terminal_open_isnt_idle() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link, _) = with_worker(dir.path());
    hub.terminal_open(open("t")).await.unwrap();

    assert_eq!(hub.status().open_terminals, 1);
    assert_eq!(hub.shutdown_if_idle().map_err(|e| e.code), Err(ErrorCode::Conflict));
}
