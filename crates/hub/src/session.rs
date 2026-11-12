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
use protocol::event::{AgentTask, EventBody};
use protocol::model::{
    AgentKind, AgentSessionId, ApprovalId, ContextUsage, PromptCache, RunId, Seq, Session, SessionId, SessionSettings,
    SessionSettingsChange, SessionStatus, ToolUseId, WorkspaceId,
};
use protocol::tail::{StreamDelta, Tail};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::error::internal;
use crate::hub::Hub;
use crate::store::{self, StoredSession};

mod cache;
mod commands;
mod ingest;
mod limits;
mod messages;
mod queue;
mod record;
mod status;
#[cfg(test)]
mod tests;
mod waits;

use commands::run_commands;
use ingest::run_ingest;
pub(crate) use messages::{Command, Item};
use queue::QueueWatch;

const LIVE_CAPACITY: usize = 4096;

#[derive(Clone, Debug)]
pub(crate) enum LiveItem {
    /// The log now reaches `seq`, with an event from the subagent `subagent` started, or the agent's own.
    Appended {
        seq: Seq,
        subagent: Option<ToolUseId>,
        /// It ended what another agent or subagent was writing, as the end of a turn ends a subagent's message, which
        /// a subscriber following only that one doesn't see.
        ended_others: bool,
    },
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
    /// been replaced, which mustn't change the status of the session its replacement is now running. `numbered` is its
    /// number among what the agent's worker sent, which the session keeps with it, so a hub that connects to the
    /// worker again drops what the worker sends it again.
    Run { run: RunId, current: bool, numbered: Option<u64> },
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

/// What a session carries on with from an earlier hub, whose agent is still running on a worker this hub has
/// connected to again.
pub(crate) struct CarriedOver {
    /// The connection this hub reached the agent's worker on.
    pub link: u64,
    pub run: RunId,
    /// The last of what the agent's worker numbered that the session recorded, with the run it's from.
    pub recorded: Option<(RunId, u64)>,
    /// Prompts the agent hadn't taken when the earlier hub went, which it may take yet.
    pub untaken: Vec<Seq>,
}

struct LiveState {
    last_seq: Seq,
    status: SessionStatus,
    tail: Tail,
    version: u64,
    /// Open approvals, by the run that asked. `None` means an earlier hub's, which can't be answered.
    pending_approvals: HashMap<ApprovalId, Option<RunId>>,
    /// What the current agent is running in the background, from what the session's record lists, since a wait the
    /// hub keeps outlives the agent, though nothing else does.
    background_tasks: Vec<AgentTask>,
    /// Whether the session's only work is a wait the hub keeps, as this hub stops, so its agent's exit cuts nothing
    /// off.
    outlives_this_hub: bool,
    /// How full the agent's context is, as the session's record says, so only a report that differs changes it.
    context: Option<ContextUsage>,
    /// What the API's cache holds of the conversation, as the session's record says.
    prompt_cache: Option<PromptCache>,
    /// Which account a launch of the agent runs under, learnt as it launches, for filing what it hears.
    hearing: Option<(RunId, limits::Hearing)>,
    /// When the newest turn started, as the session's record says, for the turn's end to carry.
    turn_started_at_ms: Option<i64>,
    /// The agent running, or being started. Events from any other run are leftovers from one that has
    /// been replaced: they go in the log, but don't speak for the session's current state, and its
    /// deltas are dropped.
    agent: Option<AgentRun>,
    /// The last of what the agent's worker numbered that the session has recorded, with the run it's from.
    recorded: Option<(RunId, u64)>,
    agent_opened: bool,
    /// What the next agent launches with.
    settings: SessionSettings,
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
    /// Held while the session's record changes and is announced, by an event being recorded or by filing which account
    /// a launch runs under, so they happen one at a time and the feed hears each copy in the order they were made.
    recording: tokio::sync::Mutex<()>,
    /// Held while a queued prompt is sent, and while the queue changes, so a change can't land between reading the
    /// prompt and taking it out of the queue, and be lost.
    queue_lock: tokio::sync::Mutex<()>,
    /// Held while the model, mode or effort is being changed, and while an agent is being started with them,
    /// so a change can't fall between the two and reach neither, and to keep an agent from starting meanwhile.
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
    /// `carried` is what it carries on with from an earlier hub, if its agent is still running.
    pub(crate) fn start(
        hub: &Arc<Hub>,
        stored: StoredSession,
        open_approvals: Vec<ApprovalId>,
        carried: Option<CarriedOver>,
    ) -> Arc<Self> {
        let has_queue = !stored.session.queued.is_empty();
        let (agent, recorded, untaken) = match carried {
            Some(CarriedOver { link, run, recorded, untaken }) => (Some(AgentRun { link, run }), recorded, untaken),
            None => (None, None, Vec::new()),
        };
        // The agent still running is the one that asked whatever is open, and runs on what it runs in the background.
        let asked_by = agent.map(|agent| agent.run);
        // As the record lists them: the agent's, if it carries on with one, or else only a wait, which outlives it.
        let background_tasks = stored.session.background_tasks.clone();
        let mut queue = QueueWatch::starting(stored.session.queue_held);
        queue.set_untaken(untaken);
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
                pending_approvals: open_approvals.into_iter().map(|id| (id, asked_by)).collect(),
                background_tasks,
                outlives_this_hub: false,
                context: stored.session.context.clone(),
                prompt_cache: stored.session.prompt_cache.clone(),
                hearing: None,
                turn_started_at_ms: stored.session.turn_started_at_ms,
                agent,
                recorded,
                agent_opened: stored.agent_opened,
                settings: stored.session.settings(),
                closed_at_ms: stored.session.closed_at_ms,
                queue,
                live: None,
            }),
            recording: tokio::sync::Mutex::new(()),
            queue_lock: tokio::sync::Mutex::new(()),
            configuring: tokio::sync::Mutex::new(()),
        });
        tokio::spawn(run_commands(session.clone(), commands_rx));
        tokio::spawn(run_ingest(session.clone(), ingest_rx));
        // So what the agent hears of its account's limits is filed under the account, as for one this hub launched.
        if let Some(agent) = agent {
            session.launched(agent.run);
        }
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

    /// Lets go of a session that's been deleted. Its two tasks each hold it, and so does everyone following it, so
    /// both tasks stop, and every follower does once what it waits on is gone.
    pub(crate) async fn end(&self) {
        self.state.lock().live = None;
        let _ = self.command(Command::End).await;
        self.send_item(Item::End).await;
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

    /// Saved only once a running agent has agreed, so what's shown is never ahead of what's true. A running agent reports
    /// a change of mode itself, and with none running, it goes in the log here, so the log says when every change was
    /// made.
    pub(crate) async fn configure(&self, options: SessionSettingsChange) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let _one_at_a_time = self.configuring.lock().await;
        let mut agent_took_it = false;
        if let Some(link) = hub.existing_worker(&self.workspace_id) {
            let request = WorkerRequest::AgentConfigure { session_id: self.id.clone(), options: options.clone() };
            agent_took_it = !matches!(link.call(request).await?, WorkerResponse::AgentNotRunning);
        }
        let was = self.state.lock().settings.clone();
        self.keep_options(&hub, options.clone()).await?;
        if let Some(mode) = options.permission_mode.filter(|mode| *mode != was.permission_mode && !agent_took_it) {
            self.record_or_log(Origin::Hub, EventBody::AgentMode { permission_mode: mode }).await;
        }
        // Another model's cache doesn't hold the conversation, so the next request finds nothing cached.
        if options.model.is_some_and(|model| model != was.model) {
            self.keep_cache(None).await;
        }
        self.announce(&hub).await
    }

    /// Saves the settings `change` sets, and launches the next agent with them.
    async fn keep_options(&self, hub: &Hub, change: SessionSettingsChange) -> Result<(), ApiError> {
        let (id, saved) = (self.id.clone(), change.clone());
        hub.blocking(move |store| store.set_session_options(&id, &saved)).await?;
        let mut state = self.state.lock();
        state.settings = state.settings.clone().changed(change);
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let closed_at = store::now_ms();
        let id = self.id.clone();
        {
            // An agent being started right now either finishes first, and is stopped below, or sees this.
            let starts = self.configuring.lock().await;
            let let_go_of = hub.blocking(move |store| store.set_session_closed(&id, Some(closed_at))).await?;
            self.state.lock().closed_at_ms = Some(closed_at);
            hub.let_go_of_bytes(&self.id, &let_go_of).await;
            // Closing it ended its waits. It's closed whether or not they come off its list.
            if let Err(e) = self.unlist_waits_holding(&starts).await {
                warn!(session = %self.id, "couldn't take a closed chat's waits off its list: {e}");
            }
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
