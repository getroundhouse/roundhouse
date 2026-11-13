//! What a worker sends its hub, as the hub reads it: the reply to each call, what the worker asks, and what its agents
//! say, which goes to their sessions, or is held until the hub says which agent each session has.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::client::{ApiError, ErrorCode, Outcome, error_chain};
use protocol::frame;
use protocol::model::{InstanceId, RunId, SessionId, WorkspaceId};
use protocol::worker::{AskAnswer, AskId, WorkerAsk, WorkerFrame};
use providers::Provider;
use tracing::{debug, warn};

use super::WorkerLink;
use crate::frames::FrameReader;
use crate::hub::Hub;
use crate::session::Item;

fn unreadable_reply(error: &serde_json::Error) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("a reply from the worker this hub can't read: {error}"))
}

/// Answers what the worker asked on a task of its own, since answering can take a call to the worker, whose reply
/// comes the way the ask did.
fn start_answering(
    hub: &Arc<Hub>,
    link: &Arc<WorkerLink>,
    workspace_id: &WorkspaceId,
    id: AskId,
    session_id: SessionId,
    ask: WorkerAsk,
) {
    let (hub, link, workspace_id) = (hub.clone(), link.clone(), workspace_id.clone());
    tokio::spawn(async move {
        let outcome = hub.answer_ask(&workspace_id, &link, session_id, ask).await.into();
        link.answer(AskAnswer { answer: id, outcome }).await;
    });
}

/// Refuses an ask, as a call this hub can't read is refused, so the agent that called the tool hears why at once.
fn refuse_unreadable(hub: &Hub, link: &Arc<WorkerLink>, id: AskId, error: &serde_json::Error) {
    let outcome = Outcome::Error(hub.unsupported(Some(error)));
    let link = link.clone();
    tokio::spawn(async move { link.answer(AskAnswer { answer: id, outcome }).await });
}

pub(super) async fn read_frames(
    hub: Arc<Hub>,
    provider: Arc<dyn Provider>,
    link: Arc<WorkerLink>,
    workspace_id: WorkspaceId,
    instance_id: InstanceId,
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
) {
    let mut incoming = FrameReader::new(reader);
    loop {
        let bytes = match incoming.next().await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(e) => {
                warn!("reading from a worker failed: {}", error_chain(&e));
                break;
            }
        };
        let parsed: WorkerFrame = match frame::decode(bytes) {
            Ok(parsed) => parsed,
            Err(e) => {
                // A reply from a newer worker, as after a rollback, ends its call rather than leaving it to wait until
                // the connection closes.
                if let Some(id) = frame::reply_id(bytes) {
                    link.calls.reply(id, Err(unreadable_reply(&e)));
                } else if let Some(id) = AskId::of_unreadable(bytes) {
                    refuse_unreadable(&hub, &link, id, &e);
                } else {
                    warn!("ignoring a worker frame that didn't parse: {e}");
                }
                continue;
            }
        };
        if let WorkerFrame::Reply { id, outcome } = parsed {
            link.calls.reply(id, outcome.into_result());
            continue;
        }
        // Taken at once, even while what the worker sends is held, since neither an ask nor the branches are about
        // which agent a session has.
        if let WorkerFrame::Ask { id, session_id, ask } = parsed {
            start_answering(&hub, &link, &workspace_id, id, session_id, ask);
            continue;
        }
        let Some(parsed) = hub.branches_frame(&workspace_id, &link, parsed).await else { continue };
        // No terminal is started by this hub yet, so none has anywhere to go.
        if let WorkerFrame::Terminal { .. } = parsed {
            continue;
        }
        let mut held = link.held.lock().await;
        match held.as_mut() {
            Some(held) => held.push(parsed),
            None => route(&hub, &link, parsed).await,
        }
    }
    debug!(%instance_id, "worker connection ended");
    // Wakes each waiting call with an error.
    link.calls.end();
    // A worker the hub left running has gone on to the next hub, and nothing of it has gone.
    if !link.left.load(Ordering::SeqCst) {
        hub.worker_gone(&workspace_id, &link, provider.as_ref(), &instance_id).await;
    }
    // With `send_replace`, because nobody may be waiting yet. A plain `send` with no receivers leaves
    // the value unset, and whoever waits later would wait forever.
    link.closed.send_replace(true);
}

/// Passes one of the agents' frames on to its session, and notes what's numbered to acknowledge once it's recorded.
pub(super) async fn route(hub: &Arc<Hub>, link: &WorkerLink, frame: WorkerFrame) {
    let (session_id, item, seq) = match frame {
        // Taken where they're read.
        WorkerFrame::Reply { .. }
        | WorkerFrame::Ask { .. }
        | WorkerFrame::Branches { .. }
        | WorkerFrame::ChildBranches { .. }
        | WorkerFrame::Terminal { .. } => return,
        WorkerFrame::AgentEvent { session_id, run, body, raw, attachments, seq } => {
            (session_id, Item::Event { run, body: Box::new(body), raw, attachments, seq }, seq)
        }
        WorkerFrame::AgentDelta { session_id, run, delta } => (session_id, Item::Delta { run, delta }, None),
        WorkerFrame::AgentContext { session_id, run, context, seq } => {
            (session_id, Item::Context { run, context, seq }, seq)
        }
        WorkerFrame::AgentCache { session_id, run, request, seq } => {
            (session_id, Item::Cache { run, request, seq }, seq)
        }
        WorkerFrame::AgentLimits { session_id, run, limits, every_models_week, seq } => {
            (session_id, Item::Limits { run, limits, every_models_week, seq }, seq)
        }
        WorkerFrame::AgentTail { session_id, run, tail } => (session_id, Item::Tail { run, tail }, None),
    };
    hub.route(&session_id, item).await;
    if let Some(seq) = seq {
        link.acks.routed(&session_id, seq);
    }
}

/// The newest run `frames` name for each session, since a later launch always has a higher ID, whatever order they
/// came in. A worker sends what each agent is writing after what it kept, which can make a replaced run's come last.
pub(super) fn newest_runs<'a>(frames: impl IntoIterator<Item = &'a WorkerFrame>) -> HashMap<SessionId, RunId> {
    let mut runs = HashMap::new();
    for (session_id, run) in frames.into_iter().filter_map(agent_of) {
        let newest = runs.entry(session_id).or_insert(run);
        *newest = (*newest).max(run);
    }
    runs
}

fn agent_of(frame: &WorkerFrame) -> Option<(SessionId, RunId)> {
    match frame {
        WorkerFrame::Reply { .. }
        | WorkerFrame::Ask { .. }
        | WorkerFrame::Branches { .. }
        | WorkerFrame::ChildBranches { .. }
        | WorkerFrame::Terminal { .. } => None,
        WorkerFrame::AgentEvent { session_id, run, .. }
        | WorkerFrame::AgentDelta { session_id, run, .. }
        | WorkerFrame::AgentContext { session_id, run, .. }
        | WorkerFrame::AgentCache { session_id, run, .. }
        | WorkerFrame::AgentLimits { session_id, run, .. }
        | WorkerFrame::AgentTail { session_id, run, .. } => Some((session_id.clone(), *run)),
    }
}

#[cfg(test)]
mod tests;
