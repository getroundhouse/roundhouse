use std::path::PathBuf;
use std::sync::{Arc, atomic::Ordering};

use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::sync::mpsc;

use super::{notifications, running::Shared};
use crate::{AgentOutput, stdio};

/// Waits for `agy` to exit, then settles what was open and says it exited. It repeats Codex's `supervise` but for what
/// it settles, and `Shared`'s `emit`, `stop` and drop repeat Codex's too, rather than being shared, so that adding
/// this adapter didn't rework Codex's throughout.
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
    let wakeup = tokio::spawn(super::wakeup::watch(shared.clone()));
    let status = child.wait().await;
    shared.gone.store(true, Ordering::SeqCst);
    wakeup.abort();
    let _ = wakeup.await;
    shared.rpc.close().await;
    stdio::drain(out).await;
    stdio::drain(events).await;
    let stderr_tail = stdio::drain(err).await.unwrap_or_default();
    super::turn::settle_all(&shared).await;
    if let Some(path) = pid_file {
        let _ = tokio::fs::remove_file(path).await;
    }
    let (code, signal) = status.map_or((None, None), |status| (status.code(), stdio::signal_of(status)));
    let requested = shared.requested.load(Ordering::SeqCst);
    if let Some(tap) = &shared.tap {
        tap.exited(code, signal, requested);
    }
    shared.emit(AgentOutput::Exited { code, signal, stderr_tail, requested }).await;
    shared.exited.send_replace(true);
}
