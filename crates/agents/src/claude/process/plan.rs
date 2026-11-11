//! Starting an agent in plan mode. Launched straight into it, the CLI leaves it for `default` once the plan is
//! approved, which asks before every tool, while entered from full access, it goes back to full access. So an agent
//! that's to plan is launched with full access, as every agent is, and asked into plan mode before anything else is
//! written to it. One that may not have gone into it is stopped, rather than left with full access, and says why.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use process::{Signal, signal_group};
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::PermissionMode;
use serde_json::json;
use tokio::process::ChildStdin;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::output::deliver;
use super::{Shared, ask, write_to};
use crate::AgentOutput;
use crate::claude::mode;

/// Asks a CLI just launched for a chat in `chat_mode` into plan mode, if that's the mode, on `stdin`, before anything
/// else is written there, so no prompt can reach it first. One that has gone already can't take a prompt either, and a
/// relaunch asks again.
pub(super) async fn start_in(
    chat_mode: PermissionMode,
    shared: &Arc<Shared>,
    stdin: &mut Option<ChildStdin>,
    output: &mpsc::Sender<AgentOutput>,
) {
    if chat_mode != PermissionMode::Plan {
        return;
    }
    let asked = ask(shared, &json!({"subtype": "set_permission_mode", "mode": mode::name(chat_mode)}));
    shared.entering_plan.store(true, Ordering::SeqCst);
    if let Err(e) = write_to(stdin, &asked.line).await {
        shared.pending_controls.lock().remove(&asked.request_id);
        shared.entering_plan.store(false, Ordering::SeqCst);
        debug!("could not ask the CLI into plan mode, since it has gone: {e:#}");
        return;
    }
    let (shared, output) = (shared.clone(), output.clone());
    // However long it takes, since a CLI that's starting runs the user's hooks first. A turn it starts meanwhile is
    // caught by `passed_on`.
    tokio::spawn(async move {
        if let Ok(Err(refusal)) = asked.answer.await
            && shared.entering_plan.swap(false, Ordering::SeqCst)
        {
            stop(&shared, &output, &refusal).await;
        }
    });
}

/// Whether `event` goes on to whoever started the agent, while it's being asked into plan mode. Its report of going
/// into plan mode doesn't, since to them, it started there. Nor does the start of a turn in any other mode, which
/// means it took a prompt before going into plan mode, and may be about to use a tool with full access, so it's
/// stopped.
pub(super) async fn passed_on(shared: &Shared, output: &mpsc::Sender<AgentOutput>, event: &EventBody) -> bool {
    if let EventBody::AgentMode { permission_mode } = event {
        return !(shared.entering_plan.swap(false, Ordering::SeqCst) && *permission_mode == PermissionMode::Plan);
    }
    let EventBody::AgentInit { permission_mode: started_in, .. } = event else { return true };
    if !shared.entering_plan.swap(false, Ordering::SeqCst) || *started_in == Some(PermissionMode::Plan) {
        return true;
    }
    stop(shared, output, "it started on a prompt first").await;
    false
}

async fn stop(shared: &Shared, output: &mpsc::Sender<AgentOutput>, why: &str) {
    warn!("the CLI didn't go into plan mode, so it's being stopped: {why}");
    let message = format!("Claude didn't go into plan mode, so it was stopped: {why}");
    // Before it's stopped, so this comes before its exit.
    deliver(shared, output, AgentOutput::event(EventBody::Notice { level: NoticeLevel::Error, message }, None)).await;
    signal_group(shared.pid.load(Ordering::SeqCst), Signal::Terminate);
}
