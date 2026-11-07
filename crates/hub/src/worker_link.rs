//! The hub's end of one worker connection.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::frame;
use protocol::model::WorkspaceId;
use protocol::worker::{WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};
use providers::{Provider, WorkerStream};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, warn};

use crate::error::{internal, provider_failed};
use crate::hub::Hub;
use crate::session::Item;

static NEXT_LINK_ID: AtomicU64 = AtomicU64::new(1);

/// A worker on this machine answers in milliseconds. Hitting this means something is wrong with it.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

type Reply = oneshot::Sender<Result<WorkerResponse, ApiError>>;

pub(crate) struct WorkerLink {
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
    pub(crate) async fn open(
        hub: Arc<Hub>,
        provider: Arc<dyn Provider>,
        workspace_id: WorkspaceId,
        instance_id: String,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        provider.start(&instance_id).await.map_err(|e| provider_failed(&e))?;
        let stream = match provider.connect(&instance_id).await {
            Ok(stream) => stream,
            Err(e) => {
                let _ = provider.kill(&instance_id).await;
                return Err(provider_failed(&e));
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
            warn!(instance_id, "stopping a worker that failed to start: {}", error_chain(&e));
        }
        Err(failure)
    }

    pub(crate) async fn call(&self, request: WorkerRequest) -> Result<WorkerResponse, ApiError> {
        let id = self.next_call.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&WorkerCall { id, request }).map_err(|e| internal(e.to_string()))?;
        let calls = self.calls.lock().clone().ok_or_else(lost)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.pending.lock().as_mut().ok_or_else(lost)?.insert(id, reply_tx);
        if calls.send(line).await.is_err() {
            if let Some(pending) = self.pending.lock().as_mut() {
                pending.remove(&id);
            }
            return Err(lost());
        }
        // Not held while waiting. A long call, like setup, would otherwise keep the worker's stdin open
        // after `close`, and it wouldn't find out that it should stop.
        drop(calls);
        reply_rx.await.map_err(|_| lost())?
    }

    pub(crate) fn is_open(&self) -> bool {
        !*self.closed.borrow() && self.calls.lock().is_some()
    }

    /// Closes the worker's stdin. It stops its agents, flushes, and exits.
    pub(crate) fn close(&self) {
        self.calls.lock().take();
    }

    /// Resolves once the worker's output has been fully read.
    pub(crate) async fn wait_closed(&self) {
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
    workspace_id: WorkspaceId,
    instance_id: String,
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
) {
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = (&mut reader).take(frame::MAX_FRAME_BYTES as u64 + 1).read_until(b'\n', &mut bytes).await;
        match read {
            Ok(0) => break,
            Ok(_) if bytes.len() > frame::MAX_FRAME_BYTES => {
                warn!("a worker sent an oversized frame");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                warn!("reading from a worker failed: {e}");
                break;
            }
        }
        let parsed: WorkerFrame = match frame::decode(&bytes) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!("ignoring a worker frame that didn't parse: {e}");
                continue;
            }
        };
        match parsed {
            WorkerFrame::Reply { id, outcome } => {
                let reply = link.pending.lock().as_mut().and_then(|pending| pending.remove(&id));
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
    drop(link.pending.lock().take());
    hub.worker_gone(&workspace_id, &link, provider.as_ref(), &instance_id).await;
    // With `send_replace`, because nobody may be waiting yet. A plain `send` with no receivers leaves
    // the value unset, and whoever waits later would wait forever.
    link.closed.send_replace(true);
}
