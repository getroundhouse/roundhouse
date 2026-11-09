//! Serving clients, on the Unix socket and over TLS for paired devices. Each connection reads its requests in order,
//! and answers them alongside one another.

mod admit;
mod dispatch;
mod network;
mod pump;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ClientFrame, HubFrame, ReplyOutcome};
use protocol::frame;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::Instant;
use tracing::{debug, warn};

use admit::{Admit, Gate};
pub(crate) use network::accept_devices;

use crate::hub::Hub;

pub(crate) const DEFAULT_READ_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_READ_EVENTS: u32 = 2000;
pub(crate) const PUMP_PAGE_EVENTS: u32 = 256;

/// Bounded, so a client that stops reading holds back only its own subscriptions.
const CONNECTION_QUEUE: usize = 1024;

pub(crate) async fn accept_loop(hub: Arc<Hub>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(serve_connection(hub.clone(), stream, Gate::local(&hub)));
            }
            Err(e) => {
                warn!("accepting a connection failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

struct Connection {
    frames: mpsc::Sender<HubFrame>,
    subscriptions: Mutex<HashMap<u64, AbortHandle>>,
    /// True once nothing more will be read, so a request that only waits, with nothing to carry on with, can stop.
    closed: Arc<watch::Sender<bool>>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        for (_, subscription) in self.subscriptions.lock().drain() {
            subscription.abort();
        }
    }
}

/// Why a connection stopped reading.
enum Ending {
    /// It closed, sent something unreadable, or was refused. What's queued for it is still sent.
    Closed,
    /// Its device was revoked, so nothing more is sent.
    Revoked,
}

async fn serve_connection<S>(hub: Arc<Hub>, stream: S, mut gate: Gate)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (read, write) = tokio::io::split(stream);
    let (frames, queued) = mpsc::channel::<HubFrame>(CONNECTION_QUEUE);
    let mut writer = tokio::spawn(write_frames(write, queued));
    let closed = Arc::new(watch::Sender::new(false));
    let connection = Arc::new(Connection { frames, subscriptions: Mutex::new(HashMap::new()), closed: closed.clone() });
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
    loop {
        line.clear();
        let read = read_line(&mut reader, &mut line, gate.max_frame_bytes(), gate.deadline()).await;
        // As well as around the whole connection, so a revoked device can't slip in a request that had already
        // arrived.
        if *revoked.borrow() {
            return Ending::Revoked;
        }
        if !read {
            return Ending::Closed;
        }
        let request: ClientFrame = match frame::decode(&line) {
            Ok(request) => request,
            Err(e) => {
                debug!("closing a connection that sent a bad frame: {e}");
                return Ending::Closed;
            }
        };
        let (outcome, close) = match gate.admit(hub, request.request).await {
            Admit::Handle(handled) => {
                let (hub, connection) = (hub.clone(), connection.clone());
                tokio::spawn(async move {
                    hub.requests.fetch_add(1, Ordering::Relaxed);
                    let outcome = match dispatch::handle(&hub, &connection, handled).await {
                        Ok(response) => ReplyOutcome::Response(response),
                        Err(error) => ReplyOutcome::Error(error),
                    };
                    hub.requests.fetch_sub(1, Ordering::Relaxed);
                    let _ = connection.frames.send(HubFrame::Reply { id: request.id, outcome }).await;
                });
                continue;
            }
            Admit::Answer(outcome) => (outcome, false),
            Admit::Refuse(outcome) => (outcome, true),
        };
        let _ = connection.frames.send(HubFrame::Reply { id: request.id, outcome }).await;
        if close {
            return Ending::Closed;
        }
    }
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

async fn write_frames<W: AsyncWrite + Unpin>(mut write: W, mut queued: mpsc::Receiver<HubFrame>) {
    while let Some(item) = queued.recv().await {
        let Ok(line) = frame::encode(&item) else { continue };
        // Flushed each time, because TLS can hold a write back until the next one.
        if write.write_all(&line).await.is_err() || write.flush().await.is_err() {
            return;
        }
    }
    // Says it's finished, which TLS needs to tell a close from a connection that was cut.
    let _ = write.shutdown().await;
}
