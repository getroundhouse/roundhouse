//! Independent client-request and server-request IDs over the same JSONL stream.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::oneshot;

use crate::AgentError;

/// Includes CLI initialization and loading project instructions; model turns have no RPC deadline.
pub(super) const REPLY_WAIT: Duration = Duration::from_secs(30);

/// Whether `error` turned the request down, so nothing came of it: Codex refused it or didn't know it, or the adapter
/// refused it before asking. Not a request that may not have reached Codex, one whose answer didn't come, or one Codex
/// took and failed on.
pub(super) fn refused(error: &AgentError) -> bool {
    matches!(error, AgentError::Refused(_) | AgentError::UnknownRequest(_))
}

/// Whether Codex refused a request for a turn because it has none going, as it says once the turn has ended. Its words
/// are all that tells this from another refusal, such as of a turn that can't be steered, and they're 0.169.0's. If
/// they change, the refusal is taken as one for another reason, which is safe, but slower to recover from.
pub(super) fn no_active_turn(error: &AgentError) -> bool {
    matches!(error, AgentError::Refused(message) if message.starts_with("no active turn"))
}

pub(super) struct Rpc {
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, AgentError>>>>,
    tap: Option<crate::tap::Tap>,
}

impl Rpc {
    pub(super) fn new(stdin: ChildStdin, tap: Option<crate::tap::Tap>) -> Self {
        Self {
            stdin: tokio::sync::Mutex::new(Some(stdin)),
            next: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            tap,
        }
    }

    pub(super) async fn ask(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        if let Err(error) = self.write(&json!({"id":id,"method":method,"params":params})).await {
            self.pending.lock().remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(REPLY_WAIT, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err(AgentError::Exited),
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(AgentError::NoAnswer)
            }
        }
    }

    pub(super) async fn write(&self, value: &Value) -> Result<(), AgentError> {
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

    pub(super) fn answered(&self, value: &Value) {
        let Some(id) = value.get("id").and_then(Value::as_u64) else { return };
        let Some(tx) = self.pending.lock().remove(&id) else { return };
        // JSON-RPC's own codes: a method Codex doesn't know, and an error inside Codex as it carried the request out.
        // Codex says every other no, such as to a turn that has ended, as an invalid request.
        let reply = if let Some(error) = value.get("error") {
            let message = error["message"].as_str().unwrap_or("Codex refused the request").to_owned();
            Err(match error["code"].as_i64() {
                Some(-32601) => AgentError::UnknownRequest(message),
                Some(-32603) => AgentError::Failed(message),
                _ => AgentError::Refused(message),
            })
        } else {
            value.get("result").cloned().ok_or_else(|| AgentError::Failed("Codex returned no RPC result".into()))
        };
        let _ = tx.send(reply);
    }

    pub(super) async fn close(&self) {
        *self.stdin.lock().await = None;
        self.pending.lock().clear();
    }
}
