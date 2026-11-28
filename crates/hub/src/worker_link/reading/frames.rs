use super::super::WorkerLink;
use crate::error::JsonProblem;
use crate::frames::FrameReader;
use crate::hub::Hub;
use crate::session::Item;
use protocol::client::{ApiError, ErrorCode, Outcome, error_chain};
use protocol::frame;
use protocol::model::{InstanceId, RunId, SessionId, WorkspaceId};
use protocol::worker::{AskAnswer, AskId, WorkerAsk, WorkerFrame};
use providers::Provider;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{Instrument as _, debug, error_span, info, warn};

fn unreadable_reply(error: &serde_json::Error) -> ApiError {
    ApiError::new(
        ErrorCode::Worker,
        format!("a reply from the worker this hub can't read: {}", JsonProblem::from(error)),
    )
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
    let asked = error_span!("ask", %session_id);
    tokio::spawn(
        async move {
            let outcome = hub.answer_ask(&workspace_id, &link, session_id, ask).await.into();
            link.answer(AskAnswer { answer: id, outcome }).await;
        }
        .instrument(asked),
    );
}

/// Refuses an ask, as a call this hub can't read is refused, so the agent that called the tool hears why at once.
fn refuse_unreadable(hub: &Hub, link: &Arc<WorkerLink>, id: AskId, error: &serde_json::Error) {
    let outcome = Outcome::Error(hub.unsupported(Some(error)));
    let link = link.clone();
    tokio::spawn(async move { link.answer(AskAnswer { answer: id, outcome }).await }.in_current_span());
}

pub(in super::super) async fn read_frames(
    hub: Arc<Hub>,
    provider: Arc<dyn Provider>,
    link: Arc<WorkerLink>,
    workspace_id: WorkspaceId,
    instance_id: InstanceId,
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
) {
    let mut incoming = FrameReader::new(reader);
    loop {
        // The worker's own timer can say its connection has dropped before the hub's does, and it then dials again.
        // That's the worker's word that this connection is no use, so the hub lets go of it for the new one.
        let read = tokio::select! {
            read = incoming.next() => read,
            () = hub.dial.dialed_again(&instance_id), if link.connection_dropped() => {
                info!(name: "worker.dialed_again", %instance_id, "A worker dialed again, so its old connection is let go");
                break;
            }
        };
        let bytes = match read {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(e) => {
                warn!(name: "worker.unread", error = error_chain(&e), "Couldn't read from a worker");
                break;
            }
        };
        let parsed = match frame::decode::<WorkerFrame>(bytes) {
            Ok(parsed) => parsed,
            Err(e) => {
                frame_unread(&hub, &link, bytes, &e);
                continue;
            }
        };
        if let WorkerFrame::Reply { id, outcome } = parsed {
            link.calls.reply(id, outcome.into_result());
            continue;
        }
        // Taken at once, even while what the worker sends is held, since neither an ask, the branches nor the changes
        // are about which agent a session has.
        if let WorkerFrame::Ask { id, session_id, ask } = parsed {
            start_answering(&hub, &link, &workspace_id, id, session_id, ask);
            continue;
        }
        let Some(parsed) = hub.branches_frame(&workspace_id, &link, parsed).await else { continue };
        let Some(parsed) = hub.changes_frame(&workspace_id, &link, parsed).await else { continue };
        // Taken at once too, since a terminal isn't an agent's.
        if let WorkerFrame::Terminal { terminal, update } = parsed {
            hub.terminal_update(&link, terminal, &update);
            continue;
        }
        if let WorkerFrame::TerminalPicture { terminal, picture } = parsed {
            hub.terminal_picture(&link, &terminal, picture);
            continue;
        }
        let mut held = link.held.lock().await;
        match held.as_mut() {
            Some(held) => held.push(parsed),
            None => route(&hub, &link, parsed).await,
        }
    }
    connection_ended(&hub, provider.as_ref(), &link, &workspace_id, &instance_id).await;
}

/// A frame from a worker that doesn't parse, which is a newer worker's, as after a rollback.
fn frame_unread(hub: &Hub, link: &Arc<WorkerLink>, bytes: &[u8], e: &serde_json::Error) {
    // A reply from a newer worker ends its call rather than leaving it to wait until the connection closes.
    if let Some(id) = frame::reply_id(bytes) {
        link.calls.reply(id, Err(unreadable_reply(e)));
    } else if let Some(id) = AskId::of_unreadable(bytes) {
        refuse_unreadable(hub, link, id, e);
    } else {
        warn!(
            name: "worker.frame_unread",
            kind = ?e.classify(),
            column = e.column(),
            "Ignoring a worker's frame that didn't parse"
        );
    }
}

/// What becomes of the worker once its stream has ended: nothing, if the hub left it for the next hub, it's waited for,
/// if its connection dropped, and otherwise it's gone.
async fn connection_ended(
    hub: &Arc<Hub>,
    provider: &dyn Provider,
    link: &Arc<WorkerLink>,
    workspace_id: &WorkspaceId,
    instance_id: &InstanceId,
) {
    debug!(name: "worker.connection_ended", %instance_id, "Worker connection ended");
    // Wakes each waiting call with an error, as for a worker that's gone, and none is sent again over the next
    // connection: what it asked for may have been done, and no request of the worker's is safe to send twice but the
    // few the hub already repeats, such as the sets of tool servers and credentials, which the next link greets the
    // worker with anyway. A prompt that was on its way is dropped, as one sent to an agent that's gone is. Each
    // caller has handled a worker that's gone, so none needs more. What's sent next waits for the worker instead, as
    // `LiveSession::deliver` says.
    link.calls.end();
    // A worker the hub left running has gone on to the next hub, and nothing of it has gone.
    if !link.left.load(Ordering::SeqCst) {
        if link.connection_dropped() {
            // No goodbye, so nothing says the worker has gone: it's waited for, and dials again.
            hub.worker_disconnected(workspace_id, link, instance_id).await;
        } else {
            hub.worker_gone(workspace_id, link, provider, instance_id).await;
        }
    }
    // With `send_replace`, because nobody may be waiting yet. A plain `send` with no receivers leaves
    // the value unset, and whoever waits later would wait forever.
    link.closed.send_replace(true);
}

/// Passes one of the agents' frames on to its session, and notes what's numbered to acknowledge once it's recorded.
pub(in super::super) async fn route(hub: &Arc<Hub>, link: &WorkerLink, frame: WorkerFrame) {
    let (session_id, item, seq) = match frame {
        // Taken where they're read.
        WorkerFrame::Reply { .. }
        | WorkerFrame::Ask { .. }
        | WorkerFrame::Branches { .. }
        | WorkerFrame::ChildBranches { .. }
        | WorkerFrame::Changes { .. }
        | WorkerFrame::Terminal { .. }
        | WorkerFrame::TerminalPicture { .. } => return,
        WorkerFrame::AgentEvent { session_id, run, body, raw, attachments, seq } => {
            (session_id, Item::Event { run, body: Box::new(body), raw, attachments, seq }, seq)
        }
        WorkerFrame::AgentDelta { session_id, run, delta } => (session_id, Item::Delta { run, delta }, None),
        WorkerFrame::AgentReport { session_id, run, report, seq } => {
            (session_id, Item::Report { run, report, seq }, seq)
        }
        WorkerFrame::AgentTail { session_id, run, tail } => (session_id, Item::Tail { run, tail }, None),
        WorkerFrame::AgentRetrying { session_id, run, retrying } => {
            (session_id, Item::Retrying { run, retrying }, None)
        }
    };
    hub.route(&session_id, item).await;
    if let Some(seq) = seq {
        link.acks.routed(&session_id, seq);
    }
}

/// The newest run `frames` name for each session, since a later launch always has a higher ID, whatever order they
/// came in. A worker sends what each agent is writing after what it kept, which can make a replaced run's come last.
pub(in super::super) fn newest_runs<'a>(
    frames: impl IntoIterator<Item = &'a WorkerFrame>,
) -> HashMap<SessionId, RunId> {
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
        | WorkerFrame::Changes { .. }
        | WorkerFrame::Terminal { .. }
        | WorkerFrame::TerminalPicture { .. } => None,
        WorkerFrame::AgentEvent { session_id, run, .. }
        | WorkerFrame::AgentDelta { session_id, run, .. }
        | WorkerFrame::AgentReport { session_id, run, .. }
        | WorkerFrame::AgentTail { session_id, run, .. }
        | WorkerFrame::AgentRetrying { session_id, run, .. } => Some((session_id.clone(), *run)),
    }
}
