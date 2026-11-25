use std::sync::Arc;

use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::sync::mpsc;

use super::{notifications, running::Shared};
use crate::stdio;
use tracing::Instrument as _;

pub(super) async fn supervise(mut child: Child, stdout: ChildStdout, stderr: ChildStderr, shared: Arc<Shared>) {
    let (tx, rx) = mpsc::channel(256);
    let out = tokio::spawn(stdio::read_json_lines(stdout, shared.tap.clone(), tx).in_current_span());
    let events = tokio::spawn(notifications::read(shared.clone(), rx).in_current_span());
    let err = tokio::spawn(stdio::read_stderr(stderr).in_current_span());
    let status = child.wait().await;
    shared.run.reaped();
    shared.run.mark_gone();
    shared.rpc.close().await;
    stdio::drain(out, shared.drain_grace).await;
    stdio::drain(events, shared.drain_grace).await;
    let stderr_tail = stdio::drain(err, shared.drain_grace).await.unwrap_or_default();
    shared.state.lock().sending = None;
    super::control::settle(&shared, None).await;
    let children: Vec<_> = shared.state.lock().subagents.runs.keys().cloned().collect();
    for call in children {
        super::control::settle(&shared, Some(&call)).await;
    }
    shared.state.lock().work.clear();
    let (code, signal) = status.map_or((None, None), |status| (status.code(), stdio::signal_of(status)));
    let requested = shared.run.requested();
    if let Some(tap) = &shared.tap {
        tap.exited(code, signal, requested);
    }
    shared.run.exit(code, signal, stderr_tail, requested).await;
}
