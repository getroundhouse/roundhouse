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

use super::running::{ChildSlot, Shared};
use super::{launch, supervise};
use crate::AgentError;
use crate::jsonrpc::{Dialect, Rpc};
use crate::run::stop_group;
use crate::starting::start_piped;

/// Restarts the underlying `agy acp` child via `session/resume` once idle so `--dangerously-skip-permissions` matches
/// `state.mode`. Callers hold `shared.controls`.
pub(super) async fn ensure_mode_process(shared: &Arc<Shared>) -> Result<(), AgentError> {
    let swap = {
        let state = shared.state.lock();
        if shared.run.is_gone() {
            return Err(AgentError::Exited);
        }
        state.needs_mode_swap().then(|| state.session_id.clone().map(|id| (state.mode, id))).flatten()
    };
    let Some((target_mode, session_id)) = swap else { return Ok(()) };
    stop_current_for_swap(shared).await;
    if shared.run.requested() {
        finish_stopped_during_swap(shared).await;
        return Err(AgentError::Exited);
    }
    spawn_swapped_child(shared, &session_id, target_mode).await
}

async fn stop_current_for_swap(shared: &Shared) {
    let exited = {
        let slot = shared.child.lock();
        slot.replacing.store(true, Ordering::SeqCst);
        slot.exited.clone()
    };
    stop_group(|| shared.run.pid(), exited).await;
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
    let context = &shared.context;
    let launched = match start_piped("Antigravity", &context.bin, args, &context.cwd, &context.env, &context.options) {
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
    shared.run.replaced(launched.pid);
    *shared.rpc_slot.lock() = rpc.clone();
    *shared.child.lock() = ChildSlot { replacing: replacing.clone(), exited: exited_rx };
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
        Ok(_) => launch::resume_after_swap(shared, session_id, target_mode).await,
        Err(error) => Err(error),
    };
    if let Err(error) = setup {
        fail_spawned_swap(shared, &error).await;
        return Err(error);
    }
    Ok(())
}

async fn finish_stopped_during_swap(shared: &Shared) {
    shared.run.mark_gone();
    super::turn::settle_all(shared).await;
    shared.run.exit(Some(0), None, Vec::new(), true).await;
}

async fn fail_unspawned_swap(shared: &Shared, error: &AgentError) {
    shared.run.mark_gone();
    super::turn::settle_all(shared).await;
    shared
        .run
        .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
        .await;
    shared.run.exit(None, None, vec![error.to_string()], false).await;
}

async fn fail_spawned_swap(shared: &Shared, error: &AgentError) {
    shared
        .run
        .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
        .await;
    let mut exited = shared.child.lock().exited.clone();
    process::signal_group(shared.run.pid(), process::Signal::Kill);
    let _ = exited.wait_for(|done| *done).await;
}
