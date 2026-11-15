use std::path::PathBuf;
use std::sync::{Arc, atomic::Ordering};

use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::sync::mpsc;

use super::{notifications, running::Shared};
use crate::{AgentOutput, stdio};

pub(super) async fn supervise(
    mut child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    shared: Arc<Shared>,
    pid_file: Option<PathBuf>,
) {
    let (tx, rx) = mpsc::channel(256);
    let out = tokio::spawn(stdio::read_json_lines(stdout, shared.tap.clone(), tx));
    let events = tokio::spawn(notifications::read(shared.clone(), rx));
    let err = tokio::spawn(stdio::read_stderr(stderr));
    let status = child.wait().await;
    shared.gone.store(true, Ordering::SeqCst);
    shared.rpc.close().await;
    stdio::drain(out).await;
    stdio::drain(events).await;
    let stderr_tail = stdio::drain(err).await.unwrap_or_default();
    shared.state.lock().sending = None;
    super::control::settle(&shared, None).await;
    let children: Vec<_> = shared.state.lock().subagents.runs.keys().cloned().collect();
    for call in children {
        super::control::settle(&shared, Some(&call)).await;
    }
    shared.state.lock().work.clear();
    if let Some(path) = pid_file {
        let _ = tokio::fs::remove_file(path).await;
    }
    let (code, signal) = status.map_or((None, None), |status| (status.code(), stdio::signal_of(status)));
    if let Some(tap) = &shared.tap {
        tap.exited(code, signal, shared.requested.load(Ordering::SeqCst));
    }
    shared
        .emit(AgentOutput::Exited { code, signal, stderr_tail, requested: shared.requested.load(Ordering::SeqCst) })
        .await;
    shared.exited.send_replace(true);
}
