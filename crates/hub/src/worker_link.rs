//! The hub's end of one worker connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ErrorCode};
use protocol::frame;
use protocol::worker::{WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};
use providers::{Provider, WorkerStream};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, warn};

use crate::Hub;
use crate::session::Item;

static NEXT_LINK_ID: AtomicU64 = AtomicU64::new(1);

/// A worker on this machine answers in milliseconds. Hitting this means something is wrong with it.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

type Reply = oneshot::Sender<Result<WorkerResponse, ApiError>>;

pub struct WorkerLink {
    /// Distinguishes this connection from an earlier one to the same workspace.
    pub id: u64,
    next_call: AtomicU64,
    /// Dropping this closes the worker's stdin, which is how it's told to stop.
    calls: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// `None` once the connection has ended, so a call can't register for a reply that won't come.
    pending: Mutex<Option<HashMap<u64, Reply>>>,
    closed: watch::Sender<bool>,
}

fn lost() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the connection to the worker was lost")
}

impl WorkerLink {
    /// Starts the instance's worker and says hello. On failure nothing is left running, so the next
    /// attempt starts clean.
    pub async fn open(
        hub: Arc<Hub>,
        provider: Arc<dyn Provider>,
        workspace_id: String,
        instance_id: String,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        let provider_error = |e: anyhow::Error| ApiError::new(ErrorCode::Worker, format!("{e:#}"));
        provider.start(&instance_id).await.map_err(provider_error)?;
        let stream = match provider.connect(&instance_id).await {
            Ok(stream) => stream,
            Err(e) => {
                let _ = provider.kill(&instance_id).await;
                return Err(provider_error(e));
            }
        };
        let (calls_tx, calls_rx) = mpsc::channel::<Vec<u8>>(256);
        let link = Arc::new(WorkerLink {
            id: NEXT_LINK_ID.fetch_add(1, Ordering::Relaxed),
            next_call: AtomicU64::new(1),
            calls: Mutex::new(Some(calls_tx)),
            pending: Mutex::new(Some(HashMap::new())),
            closed: watch::channel(false).0,
        });
        let WorkerStream { reader, writer } = stream;
        tokio::spawn(write_calls(writer, calls_rx));
        tokio::spawn(read_frames(hub, provider.clone(), link.clone(), workspace_id, instance_id.clone(), reader));

        let hello = link.call(WorkerRequest::Hello { protocol_version: PROTOCOL_VERSION });
        let failure = match tokio::time::timeout(HELLO_TIMEOUT, hello).await {
            Ok(Ok(WorkerResponse::Hello { .. })) => return Ok(link),
            Ok(Ok(other)) => ApiError::new(ErrorCode::Worker, format!("unexpected hello reply: {other:?}")),
            Ok(Err(e)) => e,
            Err(_) => ApiError::new(ErrorCode::Worker, format!("the worker didn't answer within {HELLO_TIMEOUT:?}")),
        };
        link.close();
        if let Err(e) = provider.kill(&instance_id).await {
            warn!(instance_id, "stopping a worker that failed to start: {e:#}");
        }
        Err(failure)
    }

    pub async fn call(&self, request: WorkerRequest) -> Result<WorkerResponse, ApiError> {
        let id = self.next_call.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&WorkerCall { id, request })
            .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
        let calls = self.calls.lock().unwrap().clone().ok_or_else(lost)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.pending.lock().unwrap().as_mut().ok_or_else(lost)?.insert(id, reply_tx);
        if calls.send(line).await.is_err() {
            if let Some(pending) = self.pending.lock().unwrap().as_mut() {
                pending.remove(&id);
            }
            return Err(lost());
        }
        reply_rx.await.map_err(|_| lost())?
    }

    pub fn is_open(&self) -> bool {
        !*self.closed.borrow() && self.calls.lock().unwrap().is_some()
    }

    /// Closes the worker's stdin. It stops its agents, flushes, and exits.
    pub fn close(&self) {
        self.calls.lock().unwrap().take();
    }

    /// Resolves once the worker's output has been fully read.
    pub async fn wait_closed(&self) {
        let mut closed = self.closed.subscribe();
        let _ = closed.wait_for(|done| *done).await;
    }
}

async fn write_calls(mut writer: Box<dyn tokio::io::AsyncWrite + Send + Unpin>, mut calls: mpsc::Receiver<Vec<u8>>) {
    while let Some(line) = calls.recv().await {
        if let Err(e) = writer.write_all(&line).await.and(writer.flush().await) {
            warn!("writing to a worker failed: {e}");
            break;
        }
    }
    let _ = writer.shutdown().await;
}

async fn read_frames(
    hub: Arc<Hub>,
    provider: Arc<dyn Provider>,
    link: Arc<WorkerLink>,
    workspace_id: String,
    instance_id: String,
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
) {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut reader).take(frame::MAX_FRAME_BYTES as u64 + 1).read_until(b'\n', &mut line).await;
        match read {
            Ok(0) => break,
            Ok(_) if line.len() > frame::MAX_FRAME_BYTES => {
                warn!("a worker sent an oversized frame");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                warn!("reading from a worker failed: {e}");
                break;
            }
        }
        let parsed: WorkerFrame = match frame::decode(&line) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!("ignoring a worker frame that didn't parse: {e}");
                continue;
            }
        };
        match parsed {
            WorkerFrame::Reply { id, outcome } => {
                let reply = link.pending.lock().unwrap().as_mut().and_then(|pending| pending.remove(&id));
                if let Some(reply) = reply {
                    let _ = reply.send(match outcome {
                        WorkerOutcome::Response(response) => Ok(response),
                        WorkerOutcome::Error(error) => Err(error),
                    });
                }
            }
            WorkerFrame::AgentEvent { session_id, run, body, raw } => {
                hub.route(&session_id, Item::Event { run, body, raw }).await;
            }
            WorkerFrame::AgentDelta { session_id, delta } => {
                hub.route(&session_id, Item::Delta(delta)).await;
            }
        }
    }
    debug!(instance_id, "worker connection ended");
    link.close();
    // Dropping these wakes each waiting call with an error.
    drop(link.pending.lock().unwrap().take());
    hub.worker_gone(&workspace_id, &link, provider.as_ref(), &instance_id).await;
    // With `send_replace`, because nobody may be waiting yet. A plain `send` with no receivers leaves
    // the value unset, and whoever waits later would wait forever.
    link.closed.send_replace(true);
}
