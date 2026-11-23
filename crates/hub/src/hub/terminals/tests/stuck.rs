//! A worker that never answers what the hub asks of its terminals, which holds up only the call it doesn't answer, and
//! only for as long as one that answers at once is given, or nothing, where the hub doesn't wait for its answer.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::TerminalId;
use protocol::worker::{RunningTerminal, RunningTerminalsReply, WorkerRequest, WorkerResponse};
use tokio::time::{Instant, timeout};

use crate::hub::Followed;
use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::worker_link::{ANSWERED_WITHIN, WorkerLink};

/// A worker an earlier hub left that never says which terminals it runs leaves every list of the hub's records to be
/// made meanwhile, and is taken to run none once it has had as long as one that answers at once.
#[tokio::test(start_paused = true)]
async fn a_worker_that_never_says_which_terminals_it_runs_holds_up_no_list() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|request| {
        (!matches!(request, WorkerRequest::RunningTerminals)).then_some(Ok(WorkerResponse::Ok))
    });
    serving(&hub, &link);
    let began = Instant::now();
    let carrying_on = tokio::spawn({
        let (hub, link) = (hub.clone(), link.clone());
        async move { hub.carry_on_terminals(&"w".into(), &link).await }
    });
    tokio::task::yield_now().await;

    let listed = timeout(Duration::from_secs(1), hub.records_list()).await;

    assert!(listed.expect("the list waited for the worker").is_ok());
    let carried_on = timeout(ANSWERED_WITHIN * 2, carrying_on).await.expect("it waited for the worker for ever");
    assert_eq!(carried_on.unwrap(), 0, "taken to run none");
    let waited = began.elapsed();
    assert!(waited >= ANSWERED_WITHIN && waited < ANSWERED_WITHIN + Duration::from_secs(1), "{waited:?}");
}

/// A worker an earlier hub left that never ends a shell the hub can't give a record, as where the database gives no
/// revision, is told to end it, and holds up neither a list of the hub's records nor the hub carrying on with it.
#[tokio::test(start_paused = true)]
async fn a_worker_that_never_ends_a_shell_given_no_record_holds_up_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let stopped = Arc::new(Mutex::new(Vec::<TerminalId>::new()));
    let link = WorkerLink::answering({
        let stopped = stopped.clone();
        // It's asked which terminals it runs, and then told to end the shell.
        move |request| {
            if let WorkerRequest::TerminalStop(stop) = request {
                stopped.lock().push(stop.terminal);
                return None;
            }
            let running = RunningTerminal { terminal: "t".into(), opened_at_ms: 5 };
            Some(Ok(WorkerResponse::RunningTerminals(RunningTerminalsReply { terminals: vec![running] })))
        }
    });
    serving(&hub, &link);
    let refuse =
        "CREATE TRIGGER no_rev BEFORE UPDATE ON counters WHEN OLD.name = 'rev' BEGIN SELECT RAISE(ABORT, 'no'); END";
    hub.store.holding_writes().execute_batch(refuse).unwrap();
    let carrying_on = tokio::spawn({
        let (hub, link) = (hub.clone(), link.clone());
        async move { hub.carry_on_terminals(&"w".into(), &link).await }
    });
    waiting::task::until(Duration::from_secs(1), "the shell to be told to end", || !stopped.lock().is_empty()).await;

    let listed = timeout(Duration::from_secs(1), hub.records_list()).await;

    assert!(listed.expect("the list waited for the shell to end").is_ok());
    let carried_on = timeout(Duration::from_secs(1), carrying_on).await.expect("it waited for the shell to end");
    assert_eq!(carried_on.unwrap(), 0, "it had no record to give");
    assert_eq!(*stopped.lock(), [TerminalId::from("t")]);
}

/// A worker that never answers whether a client watches its terminal holds each telling only until the limit, and
/// clients that come and go meanwhile leave one telling waiting for its turn, which tells it what's so once it has it,
/// rather than one for each.
#[tokio::test(start_paused = true)]
#[expect(clippy::disallowed_methods, reason = "tokio's clock is paused, so sleeping moves it past every limit")]
async fn a_worker_that_never_answers_whether_a_terminal_is_watched_leaves_one_telling_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let told = Arc::new(Mutex::new(Vec::new()));
    let link = WorkerLink::answering({
        let told = told.clone();
        move |request| {
            let WorkerRequest::TerminalWatch(watch) = request else { return Some(Ok(WorkerResponse::Ok)) };
            told.lock().push(watch.watching);
            None
        }
    });
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    let mut following = hub.follow([Followed::Terminal("t".into())]);
    waiting::task::until(Duration::from_secs(1), "the worker to be told", || !told.lock().is_empty()).await;

    for _ in 0..5 {
        drop(following);
        following = hub.follow([Followed::Terminal("t".into())]);
    }
    drop(following);
    tokio::time::sleep(ANSWERED_WITHIN * 20).await;

    assert_eq!(*told.lock(), [true, false], "told once more, of what's so once the first ran out");
}
