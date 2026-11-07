//! One session, while the hub has it in memory.
//!
//! Two tasks do the work. `ingest` records what the agent said, and `commands` carries out what
//! clients ask for, which means waiting on worker replies. They're separate because those replies
//! arrive on the same stream as agent output: if one task did both, waiting for a reply could stop
//! the output being read, and the reply behind it would never arrive.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use protocol::client::ApiError;
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{AgentKind, Seq, Session, SessionId, SessionStatus, WorkspaceId};
use protocol::tail::{StreamDelta, Tail};
use protocol::worker::WorkerRequest;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::warn;

use crate::error::internal;
use crate::hub::Hub;
use crate::store::{self, StoredSession};

mod commands;
mod ingest;
mod record;
mod status;
#[cfg(test)]
mod tests;

use commands::run_commands;
use ingest::run_ingest;

const LIVE_CAPACITY: usize = 4096;

/// What arrives from a worker.
pub(crate) enum Item {
    Event {
        run: u64,
        body: EventBody,
        raw: Option<String>,
    },
    Delta(StreamDelta),
    /// The worker connection ended. Everything it sent has been queued ahead of this.
    WorkerLost(u64),
    /// Resolves once everything queued ahead of it has been recorded.
    Flush(oneshot::Sender<()>),
}

pub(crate) enum Command {
    Prompt { text: String, client_msg_id: Option<String>, reply: oneshot::Sender<Result<(Seq, bool), ApiError>> },
    Approve { approval_id: String, decision: ApprovalDecision, reply: oneshot::Sender<Result<(), ApiError>> },
    Cancel { reply: oneshot::Sender<Result<(), ApiError>> },
    Stop { reply: oneshot::Sender<Result<(), ApiError>> },
}

#[derive(Clone, Debug)]
pub(crate) enum LiveItem {
    /// The log now reaches this `seq`.
    Appended(Seq),
    Delta(StreamDelta),
}

/// `version` puts every change in one order, whichever task made it, so a subscriber can tell what
/// a snapshot already includes.
#[derive(Clone, Debug)]
pub(crate) struct Live {
    pub version: u64,
    pub item: LiveItem,
}

pub(crate) struct Snapshot {
    pub last_seq: Seq,
    pub tail: Tail,
    pub version: u64,
}

/// One launch of an agent, on one connection to a worker.
#[derive(Clone, Copy, PartialEq, Eq)]
struct AgentRun {
    link: u64,
    run: u64,
}

struct LiveState {
    last_seq: Seq,
    status: SessionStatus,
    tail: Tail,
    version: u64,
    /// Open approvals, by the run that asked. 0 means an earlier hub's, which can't be answered.
    pending_approvals: HashMap<String, u64>,
    /// The agent believed to be running. Events from any other run are leftovers from one that has
    /// been replaced: they go in the log, but don't speak for the session's current state.
    agent: Option<AgentRun>,
    agent_opened: bool,
    model: Option<String>,
    permission_mode: Option<String>,
    title: Option<String>,
    closed_at_ms: Option<i64>,
    /// Only exists while someone is subscribed. It's a large buffer, and most sessions a long-running
    /// hub has touched have nobody watching.
    live: Option<broadcast::Sender<Live>>,
}

pub(crate) struct LiveSession {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    agent: AgentKind,
    agent_session_id: String,
    created_at_ms: i64,
    /// So a failed setup is mentioned once, rather than on every prompt.
    warned_about_setup: std::sync::atomic::AtomicBool,
    hub: Weak<Hub>,
    commands: mpsc::Sender<Command>,
    ingest: mpsc::Sender<Item>,
    state: Mutex<LiveState>,
    recording: tokio::sync::Mutex<()>,
    /// Held while the model and mode are being changed, and while an agent is being started with them,
    /// so a change can't fall between the two and reach neither.
    configuring: tokio::sync::Mutex<()>,
}

/// For events the hub records itself, which belong to no run of an agent.
const NO_RUN: u64 = 0;

/// Tells subscribers something changed, under the lock, so their order matches `version`.
fn publish(state: &mut LiveState, item: LiveItem) {
    state.version += 1;
    let version = state.version;
    if let Some(live) = &state.live
        && live.send(Live { version, item }).is_err()
    {
        // The last subscriber has gone.
        state.live = None;
    }
}

impl LiveSession {
    pub(crate) fn start(hub: &Arc<Hub>, stored: StoredSession, open_approvals: Vec<String>) -> Arc<Self> {
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (ingest_tx, ingest_rx) = mpsc::channel(1024);
        let session = Arc::new(LiveSession {
            id: stored.session.id.clone(),
            workspace_id: stored.session.workspace_id.clone(),
            agent: stored.session.agent,
            agent_session_id: stored.agent_session_id,
            created_at_ms: stored.session.created_at_ms,
            warned_about_setup: std::sync::atomic::AtomicBool::new(false),
            hub: Arc::downgrade(hub),
            commands: commands_tx,
            ingest: ingest_tx,
            state: Mutex::new(LiveState {
                last_seq: stored.session.last_seq,
                status: stored.session.status,
                tail: Tail::default(),
                version: 0,
                pending_approvals: open_approvals.into_iter().map(|id| (id, 0)).collect(),
                agent: None,
                agent_opened: stored.agent_opened,
                model: stored.session.model.clone(),
                permission_mode: stored.session.permission_mode.clone(),
                title: stored.session.title.clone(),
                closed_at_ms: stored.session.closed_at_ms,
                live: None,
            }),
            recording: tokio::sync::Mutex::new(()),
            configuring: tokio::sync::Mutex::new(()),
        });
        tokio::spawn(run_commands(session.clone(), commands_rx));
        tokio::spawn(run_ingest(session.clone(), ingest_rx));
        session
    }

    fn hub(&self) -> Result<Arc<Hub>, ApiError> {
        self.hub.upgrade().ok_or_else(|| internal("the hub is shutting down"))
    }

    pub(crate) async fn command(&self, command: Command) -> Result<(), ApiError> {
        self.commands.send(command).await.map_err(|_| internal("the session has stopped"))
    }

    pub(crate) async fn send_item(&self, item: Item) {
        if self.ingest.send(item).await.is_err() {
            warn!(session = %self.id, "dropped worker output for a stopped session");
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Live> {
        let mut state = self.state.lock();
        state.live.get_or_insert_with(|| broadcast::channel(LIVE_CAPACITY).0).subscribe()
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let state = self.state.lock();
        Snapshot { last_seq: state.last_seq, tail: state.tail.clone(), version: state.version }
    }

    pub(crate) fn status(&self) -> SessionStatus {
        self.state.lock().status
    }

    pub(crate) fn agent_running(&self) -> bool {
        self.state.lock().agent.is_some()
    }

    /// Saved only once a running agent has agreed, so what's shown is never ahead of what's true.
    pub(crate) async fn configure(
        &self,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let _one_at_a_time = self.configuring.lock().await;
        if let Some(link) = hub.existing_worker(&self.workspace_id) {
            let request = WorkerRequest::AgentConfigure {
                session_id: self.id.clone(),
                model: model.clone(),
                permission_mode: permission_mode.clone(),
            };
            link.call(request).await?;
        }
        let (id, saved_model, saved_mode) = (self.id.clone(), model.clone(), permission_mode.clone());
        hub.blocking(move |store| store.set_session_options(&id, saved_model.as_deref(), saved_mode.as_deref()))
            .await?;
        {
            let mut state = self.state.lock();
            state.model = model.or(state.model.take());
            state.permission_mode = permission_mode.or(state.permission_mode.take());
        }
        Ok(self.announce(&hub))
    }

    pub(crate) async fn close(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let closed_at = store::now_ms();
        let id = self.id.clone();
        {
            // An agent being started right now either finishes first, and is stopped below, or sees this.
            let _no_start_meanwhile = self.configuring.lock().await;
            hub.blocking(move |store| store.set_session_closed(&id, Some(closed_at))).await?;
            self.state.lock().closed_at_ms = Some(closed_at);
        }
        // Past anything in the queue, so a prompt waiting on setup can't outlive this.
        self.handle_stop().await?;
        Ok(self.announce(&hub))
    }

    pub(crate) async fn reopen(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let id = self.id.clone();
        hub.blocking(move |store| store.set_session_closed(&id, None)).await?;
        self.state.lock().closed_at_ms = None;
        Ok(self.announce(&hub))
    }

    fn is_closed(&self) -> bool {
        self.state.lock().closed_at_ms.is_some()
    }
}
