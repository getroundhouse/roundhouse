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
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::AbortHandle;
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
    closed: watch::Sender<bool>,
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
    let writer = tokio::spawn(write_frames(write, queued));
    let connection =
        Arc::new(Connection { frames, subscriptions: Mutex::new(HashMap::new()), closed: watch::Sender::new(false) });

    let ending = read_requests(&hub, &connection, &mut gate, BufReader::new(read)).await;
    connection.closed.send_replace(true);
    drop(connection);
    match ending {
        // Finish writing what's queued, such as the error for a version mismatch.
        Ending::Closed => {
            let _ = writer.await;
        }
        Ending::Revoked => writer.abort(),
    }
    if let Some(device) = &gate.device {
        hub.device_disconnected(device).await;
    }
    // Last, so it stops counting among the hub's clients once everything else is done.
    drop(gate);
}

async fn read_requests<R>(hub: &Arc<Hub>, connection: &Arc<Connection>, gate: &mut Gate, mut reader: R) -> Ending
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = read_line(&mut reader, &mut line, gate.max_frame_bytes(), gate.deadline());
        // Revoking comes first, so a revoked device can't slip in another request that's already arrived.
        let read = tokio::select! {
            biased;
            () = revoked(gate.device.as_mut().map(|device| &mut device.revoked)) => return Ending::Revoked,
            read = read => read,
        };
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

/// Resolves when the connection's device is revoked, and never for a connection that isn't a device's.
async fn revoked(revoked: Option<&mut oneshot::Receiver<()>>) {
    match revoked {
        Some(revoked) => {
            let _ = revoked.await;
        }
        None => std::future::pending().await,
    }
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
