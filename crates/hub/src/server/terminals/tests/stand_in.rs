//! A hub with a terminal open on a stand-in for the worker that runs it, and stand-ins for a client attached to it.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use protocol::client::{ApiError, CLIENT_PROTOCOL_VERSION, ClientFrame, ClientRequest, Hello, HubFrame};
use protocol::frame;
use protocol::terminal::{TerminalBytes, TerminalExit, TerminalScreen, TerminalSize, TerminalUpdate, UpdateIndex};
use protocol::worker::{TerminalShown, WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::super::following::Attached;
use crate::hub::Hub;
use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::server::Outgoing;
use crate::worker_link::WorkerLink;

pub(super) const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
/// Long enough for anything here on a busy machine.
pub(super) const WAIT: Duration = Duration::from_secs(10);

pub(super) fn screen() -> TerminalScreen {
    TerminalScreen { size: SIZE, bytes: b"screen".as_slice().into() }
}

/// What the worker says the terminal shows, taking in the updates before `next`.
pub(super) fn shown(next: u64) -> WorkerResponse {
    WorkerResponse::TerminalShown(TerminalShown { screen: screen(), next: UpdateIndex::new(next) })
}

pub(super) fn output(text: &str) -> TerminalUpdate {
    TerminalUpdate::Output { output: TerminalBytes::from(text.as_bytes()) }
}

pub(super) fn exited() -> TerminalUpdate {
    TerminalUpdate::Exited(TerminalExit { code: Some(0) })
}

/// The output among `updates`, joined, and the other updates, in order, since how output is split into frames depends
/// on how fast the client takes them.
pub(super) fn joined(updates: Vec<TerminalUpdate>) -> (String, Vec<TerminalUpdate>) {
    let (mut output, mut others) = (String::new(), Vec::new());
    for update in updates {
        match update {
            TerminalUpdate::Output { output: bytes } => output.push_str(std::str::from_utf8(bytes.as_slice()).unwrap()),
            other @ (TerminalUpdate::Screen(_)
            | TerminalUpdate::Resized { .. }
            | TerminalUpdate::Exited(_)
            | TerminalUpdate::Unknown) => others.push(other),
        }
    }
    (output, others)
}

/// A hub with `t` open in `w`, on a stand-in worker that answers the `n`th time it's asked what the terminal shows with
/// what `show(n)` says, and every other call with `Ok`.
pub(super) async fn open_on(
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
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    (hub, link)
}

/// What a stand-in worker that notes what it's asked was asked of a terminal's clients, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    Attached(bool),
    Show,
}

/// A hub with `t` open in `w`, on a stand-in worker that notes each time it's told whether a client is attached, and
/// asked what the terminal shows, and answers the `n`th time it's told with what `told(n)` says, or not at all if it's
/// None. It says the screen takes in nothing, and answers anything else with `Ok`.
pub(super) async fn open_noting(
    dir: &Path,
    told: impl Fn(usize) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
) -> (Arc<Hub>, Arc<parking_lot::Mutex<Vec<Asked>>>) {
    let hub = hub_with_worktree(dir);
    let (asked, noting) = (Arc::new(parking_lot::Mutex::new(Vec::new())), AtomicUsize::new(0));
    let notes = asked.clone();
    let link = WorkerLink::answering(move |request| {
        if let WorkerRequest::TerminalAttached(attached) = &request {
            notes.lock().push(Asked::Attached(attached.attached));
            told(noting.fetch_add(1, Ordering::SeqCst))
        } else if matches!(request, WorkerRequest::TerminalShow(_)) {
            notes.lock().push(Asked::Show);
            Some(Ok(shown(0)))
        } else {
            Some(Ok(WorkerResponse::Ok))
        }
    });
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    (hub, asked)
}

/// A client attached to `t`, and what it's sent, which it takes only when it reads it.
pub(super) struct Client {
    following: JoinHandle<()>,
    sent: mpsc::Receiver<Outgoing>,
}

impl Client {
    pub(super) fn attach(hub: &Arc<Hub>) -> Self {
        let following = hub.terminal_follow(&"t".into()).unwrap();
        let (frames, sent) = mpsc::channel(1024);
        let attached = Attached { subscription_id: 3, frames };
        Self { following: tokio::spawn(attached.follow(following, "t".into())), sent }
    }

    pub(super) async fn next(&mut self) -> TerminalUpdate {
        let queued = timeout(WAIT, self.sent.recv()).await.expect("an update").expect("the following to go on");
        let HubFrame::Terminal { subscription_id: 3, update } = queued.frame else { panic!("sent {:?}", queued.frame) };
        update
    }

    /// Stops following, as a client that has gone does.
    pub(super) async fn leave(self) {
        self.following.abort();
        let _ = self.following.await;
    }

    /// How many frames are queued for it that it hasn't read.
    pub(super) fn queued(&self) -> usize {
        self.sent.len()
    }

    /// Everything it's sent from here until the following ends.
    pub(super) async fn rest(mut self) -> Vec<TerminalUpdate> {
        let mut rest = Vec::new();
        while let Some(queued) = timeout(WAIT, self.sent.recv()).await.expect("the following to end") {
            let HubFrame::Terminal { subscription_id: 3, update } = queued.frame else {
                panic!("sent {:?}", queued.frame)
            };
            rest.push(update);
        }
        timeout(WAIT, self.following).await.unwrap().unwrap();
        rest
    }
}

/// A client's end of a connection to the hub, over a buffer of `buffer` bytes each way, which has said hello.
pub(super) struct Connected {
    pub(super) frames: Lines<BufReader<ReadHalf<DuplexStream>>>,
    pub(super) requests: WriteHalf<DuplexStream>,
}

impl Connected {
    pub(super) async fn to(hub: &Arc<Hub>, buffer: usize) -> Self {
        let (client, served) = tokio::io::duplex(buffer);
        let (read, write) = tokio::io::split(served);
        let gate = crate::server::admit::Gate::local(hub);
        tokio::spawn(crate::server::connection::serve_connection(hub.clone(), read, write, gate));
        let (read, requests) = tokio::io::split(client);
        let mut connected = Self { frames: BufReader::new(read).lines(), requests };
        let hello = ClientRequest::Hello(Hello {
            protocol_version: CLIENT_PROTOCOL_VERSION,
            client: "t".into(),
            oldest_protocol_version: None,
            device: None,
        });
        connected.send(0, hello).await;
        connected.next().await;
        connected
    }

    pub(super) async fn send(&mut self, id: u64, request: ClientRequest) {
        self.requests.write_all(&frame::encode(&ClientFrame { id, request }).unwrap()).await.unwrap();
    }

    pub(super) async fn next(&mut self) -> HubFrame {
        let line = timeout(WAIT, self.frames.next_line()).await.expect("a frame").unwrap().expect("the connection");
        frame::decode(line.as_bytes()).unwrap()
    }
}
