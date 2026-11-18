//! Swapping the underlying `agy acp` child process via `session/resume` once idle so `--dangerously-skip-permissions`
//! is active in full access and omitted in plan mode.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use protocol::capability::approvals::ApprovalMode;
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::AgentSessionId;
use tokio::sync::watch;

use super::running::{ChildSlot, STOP_GRACE, Shared};
use super::{launch, supervise};
use crate::jsonrpc::{Dialect, Rpc};
use crate::{AgentError, AgentOutput};

/// Restarts the underlying `agy acp` child via `session/resume` once idle so `--dangerously-skip-permissions` matches
/// `state.mode`. Callers hold `shared.controls`.
pub(super) async fn ensure_mode_process(shared: &Arc<Shared>) -> Result<(), AgentError> {
    let swap = {
        let state = shared.state.lock();
        if shared.gone.load(Ordering::SeqCst) {
            return Err(AgentError::Exited);
        }
        state.needs_mode_swap().then(|| state.session_id.clone().map(|id| (state.mode, id))).flatten()
    };
    let Some((target_mode, session_id)) = swap else { return Ok(()) };
    stop_current_for_swap(shared).await;
    if shared.requested.load(Ordering::SeqCst) {
        finish_stopped_during_swap(shared).await;
        return Err(AgentError::Exited);
    }
    spawn_swapped_child(shared, &session_id, target_mode).await
}

async fn stop_current_for_swap(shared: &Shared) {
    let (pid, mut exited) = {
        let slot = shared.child.lock();
        slot.replacing.store(true, Ordering::SeqCst);
        (slot.pid, slot.exited.clone())
    };
    if *exited.borrow() {
        return;
    }
    process::signal_group(pid, process::Signal::Terminate);
    if tokio::time::timeout(STOP_GRACE, exited.wait_for(|done| *done)).await.is_err() {
        process::signal_group(pid, process::Signal::Kill);
        let _ = exited.wait_for(|done| *done).await;
    }
}

async fn spawn_swapped_child(
    shared: &Arc<Shared>,
    session_id: &AgentSessionId,
    target_mode: ApprovalMode,
) -> Result<(), AgentError> {
    let args = shared.context.purpose.args(target_mode);
    if let Some(tap) = &shared.tap {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        tap.launched(true, Some(session_id), &owned);
    }
    let launched = match launch::launch(
        &shared.context.bin,
        args,
        &shared.context.cwd,
        &shared.context.env,
        &shared.context.options,
    ) {
        Ok(launched) => launched,
        Err(error) => {
            fail_unspawned_swap(shared, &error).await;
            return Err(error);
        }
    };
    let rpc = Arc::new(Rpc::new(launched.stdin, Dialect::JsonRpc2, "Antigravity", shared.tap.clone()));
    let replacing = Arc::new(AtomicBool::new(false));
    let child_gone = Arc::new(AtomicBool::new(false));
    let (exited_tx, exited_rx) = watch::channel(false);
    shared.pid.store(launched.pid, Ordering::SeqCst);
    *shared.rpc_slot.lock() = rpc.clone();
    *shared.child.lock() = ChildSlot { pid: launched.pid, replacing: replacing.clone(), exited: exited_rx };
    tokio::spawn(supervise::supervise(
        supervise::ChildPipes {
            child: launched.child,
            stdout: launched.stdout,
            stderr: launched.stderr,
            rpc,
            replacing,
            child_gone,
            exited_tx,
        },
        shared.clone(),
    ));
    let setup = match launch::initialize(shared).await {
        Ok(()) => launch::resume_after_swap(shared, session_id, target_mode).await,
        Err(error) => Err(error),
    };
    if let Err(error) = setup {
        fail_spawned_swap(shared, &error).await;
        return Err(error);
    }
    Ok(())
}

async fn finish_stopped_during_swap(shared: &Shared) {
    shared.gone.store(true, Ordering::SeqCst);
    super::turn::settle_all(shared).await;
    if let Some(path) = &shared.context.options.pid_file {
        let _ = tokio::fs::remove_file(path).await;
    }
    shared.emit(AgentOutput::Exited { code: Some(0), signal: None, stderr_tail: Vec::new(), requested: true }).await;
    shared.exited.send_replace(true);
}

async fn fail_unspawned_swap(shared: &Shared, error: &AgentError) {
    shared.gone.store(true, Ordering::SeqCst);
    super::turn::settle_all(shared).await;
    if let Some(path) = &shared.context.options.pid_file {
        let _ = tokio::fs::remove_file(path).await;
    }
    shared
        .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
        .await;
    shared
        .emit(AgentOutput::Exited { code: None, signal: None, stderr_tail: vec![error.to_string()], requested: false })
        .await;
    shared.exited.send_replace(true);
}

async fn fail_spawned_swap(shared: &Shared, error: &AgentError) {
    shared
        .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
        .await;
    let (pid, mut exited) = {
        let slot = shared.child.lock();
        (slot.pid, slot.exited.clone())
    };
    process::signal_group(pid, process::Signal::Kill);
    let _ = exited.wait_for(|done| *done).await;
}
