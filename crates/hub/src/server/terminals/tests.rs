//! A client's terminal on its connection, open on a hub with a stand-in for the worker that runs it.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{OnceLock, Weak};

use parking_lot::Mutex;
use protocol::client::terminals::{TerminalClose, TerminalInput, TerminalResize};
use protocol::client::{CLIENT_PROTOCOL_VERSION, ClientFrame, Hello, ReplyOutcome};
use protocol::frame;
use protocol::terminal::{TerminalBytes, TerminalScreen, TerminalSize, UpdateIndex};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::*;
use crate::hub::QUEUED_UPDATES;
use crate::hub::tests::{as_worker_of_w, hub_with_worktree, terminal_in_w};
use crate::worker_link::WorkerLink;

const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
/// Long enough for anything here on a busy machine.
const WAIT: Duration = Duration::from_secs(10);

fn screen() -> TerminalScreen {
    TerminalScreen { size: SIZE, bytes: b"screen".as_slice().into() }
}

/// What the worker says the terminal shows, taking in the updates before `next`.
fn shown(next: u64) -> WorkerResponse {
    WorkerResponse::TerminalShown(TerminalShown { screen: screen(), next: UpdateIndex::new(next) })
}

fn output(text: &str) -> TerminalUpdate {
    TerminalUpdate::Output { output: TerminalBytes::from(text.as_bytes()) }
}

fn exited() -> TerminalUpdate {
    TerminalUpdate::Exited(TerminalExit { code: Some(0) })
}

/// A hub with `t` open in `w`, on a stand-in worker that answers the `n`th time it's asked what the terminal shows with
/// what `show(n)` says, and every other call with `Ok`.
async fn open_on(
    dir: &Path,
    show: impl Fn(usize) -> Result<WorkerResponse, ApiError> + Send + 'static,
) -> (Arc<Hub>, Arc<WorkerLink>) {
    let hub = hub_with_worktree(dir);
    let shows = AtomicUsize::new(0);
    let link = WorkerLink::answering(move |request| {
        Some(if matches!(request, WorkerRequest::TerminalShow(_)) {
            show(shows.fetch_add(1, Ordering::SeqCst))
        } else {
            Ok(WorkerResponse::Ok)
        })
    });
    as_worker_of_w(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    (hub, link)
}

/// A client attached to `t`, and what it's sent.
struct Client {
    following: JoinHandle<()>,
    sent: mpsc::Receiver<HubFrame>,
}

impl Client {
    fn attach(hub: &Arc<Hub>) -> Self {
        let (link, updates) = hub.terminal_follow(&"t".into()).unwrap();
        let (frames, sent) = mpsc::channel(1024);
        let attached = Attached { subscription_id: 3, frames };
        Self { following: tokio::spawn(attached.follow(hub.clone(), link, "t".into(), updates)), sent }
    }

    async fn next(&mut self) -> TerminalUpdate {
        let frame = timeout(WAIT, self.sent.recv()).await.expect("an update").expect("the following to go on");
        let HubFrame::Terminal { subscription_id: 3, update } = frame else { panic!("sent {frame:?}") };
        update
    }

    /// Everything it's sent from here until the following ends.
    async fn rest(mut self) -> Vec<TerminalUpdate> {
        let mut rest = Vec::new();
        while let Some(frame) = timeout(WAIT, self.sent.recv()).await.expect("the following to end") {
            let HubFrame::Terminal { subscription_id: 3, update } = frame else { panic!("sent {frame:?}") };
            rest.push(update);
        }
        timeout(WAIT, self.following).await.unwrap().unwrap();
        rest
    }
}

/// Including a new size that came before the screen, which it took in.
#[tokio::test]
async fn a_client_that_attaches_gets_the_screen_then_only_what_comes_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = open_on(dir.path(), |_| Ok(shown(2))).await;
    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    let (before, after) = (TerminalSize { cols: 100, rows: 30 }, TerminalSize { cols: 90, rows: 20 });
    let updates = [
        output("old"),
        TerminalUpdate::Resized { size: before },
        output("new"),
        TerminalUpdate::Resized { size: after },
        TerminalUpdate::Unknown,
        exited(),
    ];

    for (at, update) in (0..).zip(updates) {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), update);
    }

    assert_eq!(client.rest().await, [output("new"), TerminalUpdate::Resized { size: after }, exited()]);
}

/// What the worker sends as it's asked what the terminal shows can reach the hub before the answer does, and still
/// comes after the screen, since the client follows the updates before it asks.
#[tokio::test]
async fn an_update_that_reaches_the_hub_before_the_screen_comes_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reaching: Arc<OnceLock<(Weak<Hub>, Weak<WorkerLink>)>> = Arc::default();
    let sends = reaching.clone();
    let link = WorkerLink::answering(move |request| {
        if matches!(request, WorkerRequest::TerminalShow(_)) {
            let (hub, link) = sends.get().expect("the stand-in to know its hub");
            let (hub, link) = (hub.upgrade().unwrap(), link.upgrade().unwrap());
            hub.terminal_update(&link, "t".into(), UpdateIndex::new(2), output("after"));
            return Some(Ok(shown(2)));
        }
        Some(Ok(WorkerResponse::Ok))
    });
    reaching.set((Arc::downgrade(&hub), Arc::downgrade(&link))).unwrap();
    as_worker_of_w(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();

    let mut client = Client::attach(&hub);

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(client.next().await, output("after"));
}

/// Rather than every update it missed, which nothing holds for it.
#[tokio::test]
async fn a_client_too_far_behind_gets_what_the_terminal_shows_again() {
    let dir = tempfile::tempdir().unwrap();
    let many = u64::try_from(QUEUED_UPDATES).unwrap() + 40;
    let (hub, link) = open_on(dir.path(), move |shows| Ok(shown(if shows == 0 { 0 } else { many - 2 }))).await;
    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));

    // All at once, with no await between them, so the client can't keep up.
    for at in 0..many {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), output(&at.to_string()));
    }

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(client.next().await, output(&(many - 2).to_string()));
    assert_eq!(client.next().await, output(&(many - 1).to_string()));
    hub.terminal_close(TerminalClose { id: "t".into() }).await.unwrap();
    assert_eq!(client.rest().await, [TerminalUpdate::Exited(TerminalExit { code: None })]);
}

/// As its worker has, which takes its shell with it.
#[tokio::test]
async fn a_client_hears_a_terminal_end_once_its_worker_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = open_on(dir.path(), |_| Ok(shown(0))).await;
    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));

    // As the hub does when a worker has gone.
    hub.terminals_lost(link.id).await;

    assert_eq!(client.rest().await, [TerminalUpdate::Exited(TerminalExit { code: None })]);
}

/// A worker lets go of a terminal only once its shell has ended, which it may not have said yet.
#[tokio::test]
async fn a_terminal_its_worker_has_let_go_of_ends_and_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link) = open_on(dir.path(), |_| Err(ApiError::new(ErrorCode::NotFound, "gone"))).await;

    let client = Client::attach(&hub);

    assert_eq!(client.rest().await, [TerminalUpdate::Exited(TerminalExit { code: None })]);
    assert_eq!(hub.records_list().await.unwrap().terminals, Vec::new());
}

/// Its shell runs on, so the client waits for a screen, rather than hearing it ended.
#[tokio::test(start_paused = true)]
async fn a_worker_that_cant_say_what_a_terminal_shows_is_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link) =
        open_on(
            dir.path(),
            |shows| if shows == 0 { Err(ApiError::new(ErrorCode::Worker, "no")) } else { Ok(shown(0)) },
        )
        .await;

    let mut client = Client::attach(&hub);

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(hub.records_list().await.unwrap().terminals.len(), 1);
}

/// Typing in a burst sends a request for each key, which the connection reads in order, and has to pass on in order,
/// along with new sizes, so the last a client gives is the one the terminal keeps, though each other request runs on a
/// task of its own. On several threads, where those tasks run in any order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whats_typed_and_new_sizes_reach_the_worker_in_the_order_they_were_sent() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reached = Arc::new(Mutex::new(String::new()));
    let reaching = reached.clone();
    let link = WorkerLink::answering(move |request| {
        if let WorkerRequest::TerminalInput(input) = &request {
            reaching.lock().push_str(std::str::from_utf8(input.input.as_slice()).unwrap());
        } else if let WorkerRequest::TerminalResize(resize) = &request {
            write!(reaching.lock(), "{}x,", resize.size.cols).unwrap();
        }
        Some(Ok(WorkerResponse::Ok))
    });
    as_worker_of_w(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    let (client, served) = tokio::io::duplex(1 << 20);
    let (read, write) = tokio::io::split(served);
    tokio::spawn(super::super::serve_connection(hub, read, write));
    let hello = ClientRequest::Hello(Hello {
        protocol_version: CLIENT_PROTOCOL_VERSION,
        client: "t".into(),
        oldest_protocol_version: None,
    });
    let mut lines = frame::encode(&ClientFrame { id: 0, request: hello }).unwrap();
    let mut sent = String::new();
    for (id, key) in (1..).zip(0..500_u16) {
        let request = if key % 10 == 0 {
            write!(sent, "{}x,", key + 1).unwrap();
            let size = TerminalSize { cols: key + 1, rows: 24 };
            ClientRequest::TerminalResize(TerminalResize { id: "t".into(), size })
        } else {
            write!(sent, "{key},").unwrap();
            ClientRequest::TerminalInput(TerminalInput { id: "t".into(), input: format!("{key},").as_bytes().into() })
        };
        lines.extend(frame::encode(&ClientFrame { id, request }).unwrap());
    }
    let (client_read, mut client_write) = tokio::io::split(client);

    client_write.write_all(&lines).await.unwrap();

    let mut replies = BufReader::new(client_read).lines();
    let answered = async {
        for _ in 0..=500 {
            let reply: HubFrame = frame::decode(replies.next_line().await.unwrap().unwrap().as_bytes()).unwrap();
            assert!(matches!(reply, HubFrame::Reply { outcome: ReplyOutcome::Response(_), .. }), "{reply:?}");
        }
    };
    timeout(WAIT, answered).await.expect("every request to be answered");
    assert_eq!(*reached.lock(), sent);
}
