use std::path::PathBuf;
use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

use serde_json::Value;
use tokio::io::BufReader;
use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{notifications, running::Shared};
use crate::{AgentOutput, stdio};

/// A descendant may keep its parent's pipes open after the CLI exits.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

pub(super) async fn supervise(
    mut child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    shared: Arc<Shared>,
    pid_file: Option<PathBuf>,
) {
    let (tx, rx) = mpsc::channel(256);
    let out = tokio::spawn(read(stdout, shared.clone(), tx));
    let events = tokio::spawn(notifications::read(shared.clone(), rx));
    let err = tokio::spawn(stdio::read_stderr(stderr));
    let status = child.wait().await;
    shared.gone.store(true, Ordering::SeqCst);
    shared.rpc.close().await;
    drain(out).await;
    drain(events).await;
    let stderr_tail = drain(err).await.unwrap_or_default();
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

async fn read(stdout: ChildStdout, shared: Arc<Shared>, notifications: mpsc::Sender<(Value, String)>) {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    while let Some(raw) = stdio::next_line(&mut reader, &mut buffer).await {
        if let Some(tap) = &shared.tap {
            tap.read(&raw);
        }
        let Ok(value) = serde_json::from_str::<Value>(&raw) else { continue };
        if notifications.send((value, raw)).await.is_err() {
            break;
        }
    }
}

async fn drain<T>(mut task: JoinHandle<T>) -> Option<T> {
    if let Ok(result) = tokio::time::timeout(DRAIN_GRACE, &mut task).await {
        result.ok()
    } else {
        task.abort();
        let _ = task.await;
        None
    }
}
