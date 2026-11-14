//! A cancelled turn's yielded commands must be gone before its outcome becomes visible.
use super::{running::Shared, state::ThreadId};
use crate::AgentError;
use serde_json::json;
use std::time::Duration;

pub(super) async fn stop(shared: &Shared, thread: Option<&ThreadId>) -> Result<(), AgentError> {
    let mut terminals: Vec<_> = shared.state.lock().terminals.values().cloned().collect();
    terminals.sort();
    for terminal in &terminals {
        shared.rpc.ask("thread/backgroundTerminals/terminate", json!({"threadId":thread,"processId":terminal})).await?;
    }
    if terminals.is_empty() {
        return Ok(());
    }
    let stopped = async {
        while any_running(shared, thread, &terminals).await? {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(5), stopped).await.map_err(|_| AgentError::NoAnswer)?
}

async fn any_running(shared: &Shared, thread: Option<&ThreadId>, terminals: &[String]) -> Result<bool, AgentError> {
    let mut cursor = None;
    loop {
        let mut params = json!({"threadId":thread});
        if let Some(cursor) = &cursor {
            params["cursor"] = json!(cursor);
        }
        let response = shared.rpc.ask("thread/backgroundTerminals/list", params).await?;
        let running =
            response["data"].as_array().ok_or_else(|| AgentError::Refused("Codex returned no terminal list".into()))?;
        if running.iter().any(|p| p["processId"].as_str().is_some_and(|id| terminals.iter().any(|t| t == id))) {
            return Ok(true);
        }
        cursor = response["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return Ok(false);
        }
    }
}
