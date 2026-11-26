//! Serving clients, on the Unix socket and over TLS for paired devices. Each connection reads its requests in order,
//! and answers them alongside one another, but for what `super::in_order` takes in the order it was sent.

use super::counted::Counted;
use super::in_order::{Answering, Subscribes, in_order};
use super::subscribing::Served;
use super::{admit, dispatch};

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::listener::Listener;
use parking_lot::Mutex;
use protocol::client::{ClientRequest, HubFrame, ReplyOutcome};
use protocol::frame;

use crate::spans;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Notify, OwnedSemaphorePermit, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{Instrument as _, debug, warn};

use admit::{Admit, Gate, Greeting};

use crate::hub::Hub;

/// How many events a page a subscriber is fed holds at most, up to [`protocol::client::log::ReadLimit::BYTES`].
pub(crate) const PUMP_PAGE_EVENTS: u32 = 256;

/// The ID the next connection is given.
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(0);

/// Bounded, so a client that stops reading holds back only its own subscriptions.
const CONNECTION_QUEUE: usize = 1024;

#[expect(
    clippy::disallowed_methods,
    reason = "the pause after a connection that couldn't be accepted, for what it needed to come free"
)]
pub(crate) async fn accept_loop(hub: Arc<Hub>, mut listener: Listener) {
    loop {
        match listener.accept().await {
            Ok((read, write)) => {
                tokio::spawn(serve_connection(hub.clone(), read, write, Gate::local(&hub)));
            }
            Err(e) => {
                warn!(name: "client.connection_failed", error = %e, "Couldn't accept a connection");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

pub(super) struct Connection {
    /// Which it is, of every connection this hub has served.
    pub(super) id: u64,
    pub(super) frames: mpsc::Sender<Outgoing>,
    /// Each ended as it's dropped, which happens as the connection does.
    pub(super) subscriptions: Mutex<HashMap<u64, Served>>,
    /// True once nothing more will be read, so a request that only waits, with nothing to carry on with, can stop.
    pub(super) closed: Arc<watch::Sender<bool>>,
    /// Closes it, from what feeds one of its subscriptions.
    pub(super) closer: Closer,
}

/// Closes a connection, as the hub does when it can't send one of its subscriptions what it needs, since no frame tells
/// a client that one subscription has ended, and a client whose connection closes follows everything again. What feeds
/// a subscription holds this rather than the connection, which would keep the connection's subscriptions alive. The
/// connection stops reading, sends what's queued, and ends, and every subscription on it with it.
#[derive(Clone, Default)]
pub(super) struct Closer(Arc<Notify>);

impl Closer {
    pub(super) fn close(&self) {
        self.0.notify_one();
    }

    /// Once [`Closer::close`] has been called, before or since.
    async fn closing(&self) {
        self.0.notified().await;
    }
}

/// A frame queued for a connection, and what it holds until it has been written, as a terminal's follower holds its
/// room for its next frame.
#[derive(Debug)]
pub(super) struct Outgoing {
    pub(super) frame: HubFrame,
    pub(super) written: Option<OwnedSemaphorePermit>,
}

impl From<HubFrame> for Outgoing {
    fn from(frame: HubFrame) -> Self {
        Self { frame, written: None }
    }
}

/// Why a connection stopped reading.
enum Ending {
    /// It closed, sent something unreadable, or was refused, or the hub closed it. What's queued for it is still sent.
    Closed,
    /// Its device was revoked, so nothing more is sent.
    Revoked,
}

pub(super) async fn serve_connection(
    hub: Arc<Hub>,
    read: impl AsyncRead + Unpin,
    write: impl AsyncWrite + Unpin + Send + 'static,
    mut gate: Gate,
) {
    let (frames, queued) = mpsc::channel::<Outgoing>(CONNECTION_QUEUE);
    let mut writer = tokio::spawn(write_frames(write, queued));
    let closed = Arc::new(watch::Sender::new(false));
    let id = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
    let subscriptions = Mutex::new(HashMap::new());
    let connection =
        Arc::new(Connection { id, frames, subscriptions, closed: closed.clone(), closer: Closer::default() });
    let revoked = gate.revoked();
    let mut revoking = revoked.clone();

    let serving = serve_requests(&hub, connection, &mut gate, BufReader::new(read), &revoked, &mut writer);
    // Around everything the connection does, so revoking closes it however it's waiting: on the device to send, on
    // room to queue a reply, or on a device that has stopped reading to take what's queued.
    let revoked = tokio::select! {
        biased;
        Ok(_) = revoking.wait_for(|revoked| *revoked) => true,
        ending = serving => matches!(ending, Ending::Revoked),
    };
    closed.send_replace(true);
    if revoked {
        // Nothing more is sent, and the stream closes as its halves are dropped.
        writer.abort();
    }
    if let Some(device) = &gate.device {
        hub.device_disconnected(device).await;
    }
    // Last, so it stops counting among the hub's clients once everything else is done.
    drop(gate);
}

/// Reads and answers requests until the connection ends, and then, unless its device was revoked, sends what's
/// queued, such as the error for a version mismatch.
async fn serve_requests<R>(
    hub: &Arc<Hub>,
    connection: Arc<Connection>,
    gate: &mut Gate,
    reader: R,
    revoked: &watch::Receiver<bool>,
    writer: &mut JoinHandle<()>,
) -> Ending
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let ending = read_requests(hub, &connection, gate, reader, revoked).await;
    connection.closed.send_replace(true);
    // The writer finishes once nothing can queue a frame, and this is one thing that can.
    drop(connection);
    if let Ending::Closed = ending {
        let _ = writer.await;
    }
    ending
}

async fn read_requests<R>(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    gate: &mut Gate,
    mut reader: R,
    revoked: &watch::Receiver<bool>,
) -> Ending
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    // Dropped as the connection stops reading, which ends its task once it has taken each `Subscribe` read before.
    let subscribes = Subscribes::start(hub, connection);
    loop {
        line.clear();
        let read = tokio::select! {
            read = read_line(&mut reader, &mut line, gate.max_frame_bytes(), gate.deadline()) => read,
            () = connection.closer.closing() => return Ending::Closed,
        };
        // As well as around the whole connection, so a revoked device can't slip in a request that had already
        // arrived.
        if *revoked.borrow() {
            return Ending::Revoked;
        }
        if !read {
            return Ending::Closed;
        }
        let request = match frame::read_call::<ClientRequest>(&line) {
            Ok(request) => request,
            Err(e) => {
                debug!(
                    name: "client.frame_unread",
                    kind = ?e.classify(),
                    column = e.column(),
                    "Closing a connection that sent a bad frame"
                );
                return Ending::Closed;
            }
        };
        let (asked, unreadable) = match request.request {
            Ok(asked) => (asked, None),
            Err(error) => (ClientRequest::Unknown, Some(error)),
        };
        let (outcome, close) = match gate.admit(hub, asked).await {
            Admit::Handle(handled) => {
                let read = match unreadable {
                    Some(error) => Err(error),
                    None => Ok(handled),
                };
                answer(hub, connection, &subscribes, gate.greeting(), request.id, read).await;
                continue;
            }
            Admit::Answer(outcome) => (outcome, false),
            Admit::Refuse(outcome) => (outcome, true),
        };
        let _ = connection.frames.send(HubFrame::Reply { id: request.id, outcome }.into()).await;
        if close {
            return Ending::Closed;
        }
    }
}

/// Answers call `id`, which asked `read` of a connection whose hello said `greeting`: one taken in order before the
/// next request is read, and any other on a task of its own.
async fn answer(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    subscribes: &Subscribes,
    greeting: Greeting,
    id: u64,
    read: Result<ClientRequest, serde_json::Error>,
) {
    let (hub, connection) = (hub.clone(), connection.clone());
    // Then held by the request's task, so the count comes down however the request ends, even if the connection stops
    // reading before the task starts, as it can while typing waits for room at its worker.
    let under_way = Counted::request(&hub);
    let span = spans::request(connection.id, id);
    let answering = match read {
        Ok(ClientRequest::Subscribe(asked)) => {
            return subscribes.queue(&connection, id, asked, under_way).await;
        }
        read => in_order(&hub, &connection, read).instrument(span.clone()).await,
    };
    // On a task of its own, which outlasts the connection, so a request the hub has read is carried out whether or not
    // its client is still there, as a prompt sent just before a client quits is. A save of a tool server lets go of
    // itself instead while its server is asked, as `ToolServerSave` says.
    tokio::spawn(
        async move {
            let answered = match answering {
                Answering::Queued(queued) => queued.await,
                Answering::OnATask(Ok(request)) => dispatch::handle(&hub, &connection, greeting, request).await,
                Answering::OnATask(Err(error)) => Err(hub.unsupported(Some(&error))),
            };
            let outcome = match answered {
                Ok(response) => ReplyOutcome::Response(response),
                Err(error) => ReplyOutcome::Error(error),
            };
            drop(under_way);
            let _ = connection.frames.send(HubFrame::Reply { id, outcome }.into()).await;
        }
        .instrument(span),
    );
}

/// Reads one line into `line`. False once there's nothing more to read: the connection closed or failed, the line
/// was too long, or it didn't arrive by the deadline.
async fn read_line<R>(reader: &mut R, line: &mut Vec<u8>, limit: usize, deadline: Option<Instant>) -> bool
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut limited = reader.take(limit as u64 + 1);
    let read = limited.read_until(b'\n', line);
    let read = match deadline {
        Some(deadline) => tokio::time::timeout_at(deadline, read).await.unwrap_or(Ok(0)),
        None => read.await,
    };
    matches!(read, Ok(n) if n > 0 && line.len() <= limit)
}

/// Writes each frame queued, and only then lets go of what it holds.
async fn write_frames<W: AsyncWrite + Unpin>(mut write: W, mut queued: mpsc::Receiver<Outgoing>) {
    while let Some(Outgoing { frame, written }) = queued.recv().await {
        let Ok(line) = frame::encode(&frame) else { continue };
        // Flushed each time, because TLS can hold a write back until the next one.
        if write.write_all(&line).await.is_err() || write.flush().await.is_err() {
            return;
        }
        drop(written);
    }
    // Says it's finished, which TLS needs to tell a close from a connection that was cut.
    let _ = write.shutdown().await;
}
