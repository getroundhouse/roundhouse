use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::AgentError;

/// Includes CLI initialization and loading project instructions; model turns have no RPC deadline.
pub(crate) const REPLY_WAIT: Duration = Duration::from_secs(30);

pub(crate) type RpcReply = Result<Value, AgentError>;

/// Whether outgoing messages include `"jsonrpc": "2.0"`. ACP requires it; Codex's app-server omits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    Bare,
    JsonRpc2,
}

/// Whether `error` turned the request down, so nothing came of it: the CLI refused it or didn't know it, or the
/// adapter refused it before asking. Not a request that may not have reached the CLI, one whose answer didn't come, or
/// one the CLI took and failed on.
pub(crate) fn refused(error: &AgentError) -> bool {
    matches!(error, AgentError::Refused(_) | AgentError::UnknownRequest(_))
}

pub(crate) struct Rpc {
    stdin: tokio::sync::Mutex<Option<Box<dyn AsyncWrite + Unpin + Send>>>,
    dialect: Dialect,
    peer: &'static str,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<RpcReply>>>,
    tap: Option<crate::tap::Tap>,
}

impl Rpc {
    pub(crate) fn new(
        stdin: impl AsyncWrite + Unpin + Send + 'static,
        dialect: Dialect,
        peer: &'static str,
        tap: Option<crate::tap::Tap>,
    ) -> Self {
        Self {
            stdin: tokio::sync::Mutex::new(Some(Box::new(stdin))),
            dialect,
            peer,
            next: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            tap,
        }
    }

    /// Allocates the next client request ID for a caller that tracks the response on the ordered event stream.
    pub(crate) fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Writes a client request frame with a pre-allocated `id` without registering a oneshot waiter.
    pub(crate) async fn write_request(&self, id: u64, method: &str, params: Value) -> Result<(), AgentError> {
        self.write(&self.framed(json!({ "id": id, "method": method, "params": params }))).await
    }

    /// Sends a request without awaiting its reply, returning its allocated ID and reply receiver for turn-spanning
    /// calls.
    pub(crate) async fn send_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, oneshot::Receiver<RpcReply>), AgentError> {
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        if let Err(error) = self.write_request(id, method, params).await {
            self.pending.lock().remove(&id);
            return Err(error);
        }
        Ok((id, rx))
    }

    pub(crate) async fn ask(&self, method: &str, params: Value) -> RpcReply {
        let (id, rx) = self.send_request(method, params).await?;
        match tokio::time::timeout(REPLY_WAIT, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err(AgentError::Exited),
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(AgentError::NoAnswer)
            }
        }
    }

    pub(crate) async fn notify(&self, method: &str, params: Value) -> Result<(), AgentError> {
        self.write(&self.framed(json!({ "method": method, "params": params }))).await
    }

    pub(crate) async fn reply(&self, id: Value, result: Value) -> Result<(), AgentError> {
        self.write(&self.framed(json!({ "id": id, "result": result }))).await
    }

    /// Answers the peer's request `id` as one this side has no method for, JSON-RPC's -32601, saying `why`.
    pub(crate) async fn refuse(&self, id: Value, why: &str) -> Result<(), AgentError> {
        self.write(&self.framed(json!({ "id": id, "error": { "code": -32601, "message": why } }))).await
    }

    /// `message` with the `"jsonrpc": "2.0"` member, in the dialect that has one.
    fn framed(&self, mut message: Value) -> Value {
        if self.dialect == Dialect::JsonRpc2 {
            message["jsonrpc"] = json!("2.0");
        }
        message
    }

    pub(crate) async fn write(&self, value: &Value) -> Result<(), AgentError> {
        let mut stdin = self.stdin.lock().await;
        let stdin = stdin.as_mut().ok_or(AgentError::Exited)?;
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        if let Some(tap) = &self.tap {
            tap.wrote(value);
        }
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Decodes a JSON-RPC response frame into `(id, reply)` when `value` has no `method` and carries a numeric/string
    /// `u64` request ID.
    pub(crate) fn decode_reply(&self, value: &Value) -> Option<(u64, RpcReply)> {
        if value.get("method").is_some() {
            return None;
        }
        let id = request_id(value)?;
        let reply = if let Some(error) = value.get("error") {
            let fallback = format!("{} refused the request", self.peer);
            let message = error["message"].as_str().unwrap_or(&fallback).to_owned();
            Err(match error["code"].as_i64() {
                Some(-32601) => AgentError::UnknownRequest(message),
                Some(-32603) => AgentError::Failed(message),
                _ => AgentError::Refused(message),
            })
        } else {
            value
                .get("result")
                .cloned()
                .ok_or_else(|| AgentError::Failed(format!("{} returned no RPC result", self.peer)))
        };
        Some((id, reply))
    }

    /// Routes `value` to a waiting caller if it is a JSON-RPC response (no `method` field). Returns `true` when
    /// `value` is a response rather than a notification or server-initiated request.
    pub(crate) fn answered(&self, value: &Value) -> bool {
        if value.get("method").is_some() {
            return false;
        }
        let Some((id, reply)) = self.decode_reply(value) else {
            return value.get("result").is_some() || value.get("error").is_some();
        };
        if let Some(tx) = self.pending.lock().remove(&id) {
            let _ = tx.send(reply);
        }
        true
    }

    pub(crate) async fn close(&self) {
        *self.stdin.lock().await = None;
        self.pending.lock().clear();
    }
}

fn request_id(value: &Value) -> Option<u64> {
    let id = value.get("id")?;
    id.as_u64().or_else(|| id.as_str()?.parse::<u64>().ok())
}
