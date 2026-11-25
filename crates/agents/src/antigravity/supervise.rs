use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::sync::{mpsc, watch};

use super::{notifications, running::Shared};
use crate::jsonrpc::Rpc;
use crate::stdio;
use tracing::Instrument as _;

pub(super) struct ChildPipes {
    pub(super) child: Child,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: ChildStderr,
    pub(super) rpc: Arc<Rpc>,
    pub(super) replacing: Arc<AtomicBool>,
    pub(super) child_gone: Arc<AtomicBool>,
    pub(super) exited_tx: watch::Sender<bool>,
}

/// Waits for one `agy` child to exit. When `replacing` is set for a mode-switch respawn, settles only that child's
/// background tasks without ending the run; otherwise settles the run and ends it.
pub(super) async fn supervise(pipes: ChildPipes, shared: Arc<Shared>) {
    let ChildPipes { mut child, stdout, stderr, rpc, replacing, child_gone, exited_tx } = pipes;
    let (tx, rx) = mpsc::channel(256);
    let out = tokio::spawn(stdio::read_json_lines(stdout, shared.tap.clone(), tx).in_current_span());
    let events = tokio::spawn(notifications::read(shared.clone(), rx).in_current_span());
    let err = tokio::spawn(stdio::read_stderr(stderr).in_current_span());
    let wakeup = tokio::spawn(super::wakeup::watch(shared.clone(), child_gone.clone()).in_current_span());
    let status = child.wait().await;
    shared.run.reaped();
    child_gone.store(true, Ordering::SeqCst);
    let replaced = replacing.load(Ordering::SeqCst);
    if !replaced {
        shared.run.mark_gone();
    }
    wakeup.abort();
    let _ = wakeup.await;
    rpc.close().await;
    stdio::drain(out, shared.context.options.drain_grace).await;
    stdio::drain(events, shared.context.options.drain_grace).await;
    let stderr_tail = stdio::drain(err, shared.context.options.drain_grace).await.unwrap_or_default();
    let (code, signal) = status.map_or((None, None), |status| (status.code(), stdio::signal_of(status)));
    if replaced {
        super::turn::settle_replaced_child(&shared).await;
        if let Some(tap) = &shared.tap {
            tap.exited(code, signal, true);
        }
        exited_tx.send_replace(true);
        return;
    }
    super::turn::settle_all(&shared).await;
    let requested = shared.run.requested();
    if let Some(tap) = &shared.tap {
        tap.exited(code, signal, requested);
    }
    shared.run.exit(code, signal, stderr_tail, requested).await;
    exited_tx.send_replace(true);
}
