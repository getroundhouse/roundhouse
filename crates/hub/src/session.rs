//! One session, while the hub has it in memory.
//!
//! Two tasks do the work. `ingest` records what the agent said, and `commands` carries out what
//! clients ask for, which means waiting on worker replies. They're separate because those replies
//! arrive on the same stream as agent output: if one task did both, waiting for a reply could stop
//! the output being read, and the reply behind it would never arrive.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use protocol::attachment::NewAttachment;
use protocol::client::ApiError;
use protocol::event::{AgentTask, ApprovalDecision, EventBody};
use protocol::model::{
    AgentKind, AgentSessionId, ApprovalId, AttachmentId, ClientMessageId, ContextUsage, RunId, Seq, Session, SessionId,
    SessionOptions, SessionStatus, WorkspaceId,
};
use protocol::tail::{StreamDelta, Tail};
use protocol::worker::WorkerRequest;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::warn;

use crate::error::internal;
use crate::hub::Hub;
use crate::store::{self, StoredSession};

mod commands;
mod ingest;
mod limits;
mod queue;
mod record;
mod status;
#[cfg(test)]
mod tests;

use commands::run_commands;
use ingest::run_ingest;
use queue::QueueWatch;

const LIVE_CAPACITY: usize = 4096;

/// What arrives from a worker.
pub(crate) enum Item {
    Event {
        run: RunId,
        body: EventBody,
        raw: Option<String>,
        /// Images `body` names, which are kept before it's recorded.
        attachments: Vec<NewAttachment>,
    },
    Delta {
        run: RunId,
        delta: StreamDelta,
    },
    /// How full the agent's context is now. It comes in the queue with the agent's events, so it's never taken for
    /// a later run's, or kept after the run has gone.
    Context {
        run: RunId,
        context: ContextUsage,
    },
    /// The worker connection ended. Everything it sent has been queued ahead of this.
    WorkerLost(u64),
    /// Resolves once everything queued ahead of it has been recorded.
    Flush(oneshot::Sender<()>),
}

pub(crate) enum Command {
    Prompt {
        text: String,
        client_msg_id: Option<ClientMessageId>,
        attachments: Vec<AttachmentId>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    },
    Approve {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    Cancel {
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    Stop {
        reply: oneshot::Sender<Result<(), ApiError>>,
    },
    /// Sends the first queued prompt, if the agent is ready for it by the time this runs.
    SendQueued,
    /// Sends the queued prompt with this ID now, whether or not the agent is ready for it.
    SendQueuedNow {
        client_msg_id: ClientMessageId,
        reply: oneshot::Sender<Result<Session, ApiError>>,
    },
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

/// Where an event comes from, which decides what it may change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// Something the hub knows itself.
    Hub,
    /// One launch of the agent, from it or about it. `current` is false for a leftover from one that's
    /// been replaced, which mustn't change the status of the session its replacement is now running.
    Run { run: RunId, current: bool },
}

impl Origin {
    fn run(self) -> Option<RunId> {
        match self {
            Origin::Hub => None,
            Origin::Run { run, .. } => Some(run),
        }
    }

    fn speaks_for_session(self) -> bool {
        match self {
            Origin::Hub => true,
            Origin::Run { current, .. } => current,
        }
    }
}

/// One launch of an agent, on one connection to a worker.
#[derive(Clone, Copy, PartialEq, Eq)]
struct AgentRun {
    link: u64,
    run: RunId,
}

struct LiveState {
    last_seq: Seq,
    status: SessionStatus,
    tail: Tail,
    version: u64,
    /// Open approvals, by the run that asked. `None` means an earlier hub's, which can't be answered.
    pending_approvals: HashMap<ApprovalId, Option<RunId>>,
    /// What the current agent is running in the background. None of it outlives the agent, so it starts empty.
    background_tasks: Vec<AgentTask>,
    /// How full the agent's context is, as the session's record says, so only a report that differs changes it.
    context: Option<ContextUsage>,
    /// Which account a launch of the agent runs under, once it has heard of a limit, for filing what it hears.
    hearing: Option<(RunId, limits::Hearing)>,
    /// When the newest turn started, as the session's record says, for the turn's end to carry.
    turn_started_at_ms: Option<i64>,
    /// The agent running, or being started. Events from any other run are leftovers from one that has
    /// been replaced: they go in the log, but don't speak for the session's current state, and its
    /// deltas are dropped.
    agent: Option<AgentRun>,
    agent_opened: bool,
    /// What the next agent launches with.
    options: SessionOptions,
    closed_at_ms: Option<i64>,
    /// Whether the agent can take a queued prompt yet.
    queue: QueueWatch,
    /// Only exists while someone is subscribed. It's a large buffer, and most sessions a long-running
    /// hub has touched have nobody watching.
    live: Option<broadcast::Sender<Live>>,
}

pub(crate) struct LiveSession {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    agent: AgentKind,
    agent_session_id: AgentSessionId,
    /// So a failed setup is mentioned once, rather than on every prompt.
    warned_about_setup: std::sync::atomic::AtomicBool,
    hub: Weak<Hub>,
    commands: mpsc::Sender<Command>,
    ingest: mpsc::Sender<Item>,
    state: Mutex<LiveState>,
    recording: tokio::sync::Mutex<()>,
    /// Held while a queued prompt is sent, and while the queue changes, so a change can't land between reading the
    /// prompt and taking it out of the queue, and be lost.
    queue_lock: tokio::sync::Mutex<()>,
    /// Held while the model, mode or effort is being changed, and while an agent is being started with them,
    /// so a change can't fall between the two and reach neither.
    configuring: tokio::sync::Mutex<()>,
}

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
    pub(crate) fn start(hub: &Arc<Hub>, stored: StoredSession, open_approvals: Vec<ApprovalId>) -> Arc<Self> {
        let has_queue = !stored.session.queued.is_empty();
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (ingest_tx, ingest_rx) = mpsc::channel(1024);
        let session = Arc::new(LiveSession {
            id: stored.session.id.clone(),
            workspace_id: stored.session.workspace_id.clone(),
            agent: stored.session.agent,
            agent_session_id: stored.agent_session_id,
            warned_about_setup: std::sync::atomic::AtomicBool::new(false),
            hub: Arc::downgrade(hub),
            commands: commands_tx,
            ingest: ingest_tx,
            state: Mutex::new(LiveState {
                last_seq: stored.session.last_seq,
                status: stored.session.status,
                tail: Tail::default(),
                version: 0,
                pending_approvals: open_approvals.into_iter().map(|id| (id, None)).collect(),
                background_tasks: Vec::new(),
                context: stored.session.context.clone(),
                hearing: None,
                turn_started_at_ms: stored.session.turn_started_at_ms,
                agent: None,
                agent_opened: stored.agent_opened,
                options: stored.session.options(),
                closed_at_ms: stored.session.closed_at_ms,
                queue: QueueWatch::starting(stored.session.queue_held),
                live: None,
            }),
            recording: tokio::sync::Mutex::new(()),
            queue_lock: tokio::sync::Mutex::new(()),
            configuring: tokio::sync::Mutex::new(()),
        });
        tokio::spawn(run_commands(session.clone(), commands_rx));
        tokio::spawn(run_ingest(session.clone(), ingest_rx));
        // A queue the last hub left ready to send.
        if has_queue && session.ready_for_queue() {
            session.wake_queue();
        }
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
    pub(crate) async fn configure(&self, options: SessionOptions) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let _one_at_a_time = self.configuring.lock().await;
        if let Some(link) = hub.existing_worker(&self.workspace_id) {
            link.call(WorkerRequest::AgentConfigure { session_id: self.id.clone(), options: options.clone() }).await?;
        }
        self.keep_options(&hub, options).await?;
        self.announce(&hub).await
    }

    /// Saves the settings `newer` changes, and launches the next agent with them.
    async fn keep_options(&self, hub: &Hub, newer: SessionOptions) -> Result<(), ApiError> {
        let (id, saved) = (self.id.clone(), newer.clone());
        hub.blocking(move |store| store.set_session_options(&id, &saved)).await?;
        let mut state = self.state.lock();
        state.options = std::mem::take(&mut state.options).merged(newer);
        Ok(())
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
        // Past anything in the queue, so a prompt waiting on setup can't outlive this. It's closed whether
        // or not the agent stopped cleanly, so everyone hears that first.
        let stopped = self.handle_stop().await;
        let announced = self.announce(&hub).await;
        stopped?;
        announced
    }

    pub(crate) async fn reopen(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let id = self.id.clone();
        hub.blocking(move |store| store.set_session_closed(&id, None)).await?;
        self.state.lock().closed_at_ms = None;
        self.announce(&hub).await
    }

    fn is_closed(&self) -> bool {
        self.state.lock().closed_at_ms.is_some()
    }
}
