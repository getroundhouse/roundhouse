use super::controls::Askable;
use super::{commands, ingest, limits, messages, progress};
use crate::attachment_files::FileFor;
use crate::error::internal;
use crate::hub::Hub;
use crate::store;
use crate::store::StoredSession;
use commands::run_commands;
use ingest::run_ingest;
use messages::{Command, Item};
use parking_lot::Mutex;
use progress::Progress;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{
    AgentKind, AgentSessionId, ApprovalId, ContextUsage, PromptCache, RunId, Seq, Session, SessionId, SessionSettings,
    SessionSettingsChange, SessionStatus, ToolUseId, WorkspaceId,
};
use protocol::tail::{StreamDelta, Tail};
use protocol::worker::Delivery;
use std::sync::{Arc, Weak};
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

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
    /// The tail changed by more than a delta says, as when the hub takes what a worker kept of what its agent is
    /// writing, so each subscriber is sent it whole.
    Rewritten,
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
    pub(super) fn run(self) -> Option<RunId> {
        match self {
            Origin::Hub => None,
            Origin::Run { run, .. } => Some(run),
        }
    }

    pub(super) fn speaks_for_session(self) -> bool {
        match self {
            Origin::Hub => true,
            Origin::Run { current, .. } => current,
        }
    }
}

/// One launch of an agent, on one connection to a worker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct AgentRun {
    pub(super) link: u64,
    pub(super) run: RunId,
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

pub(super) struct LiveState {
    pub(super) last_seq: Seq,
    /// Where the agent has got to, as the session's record says, which only recording an event changes.
    pub(super) progress: Progress,
    pub(super) tail: Tail,
    pub(super) version: u64,
    /// How full the agent's context is, as the session's record says, so only a report that differs changes it.
    pub(super) context: Option<ContextUsage>,
    /// What the API's cache holds of the conversation, as the session's record says.
    pub(super) prompt_cache: Option<PromptCache>,
    /// Which account a launch of the agent runs under, learnt as it launches, for filing what it hears.
    pub(super) hearing: Option<(RunId, limits::Hearing)>,
    /// The agent running, or being started. Events from any other run are leftovers from one that has
    /// been replaced: they go in the log, but don't speak for the session's current state, and its
    /// deltas are dropped.
    pub(super) agent: Option<AgentRun>,
    /// The last of what the agent's worker numbered that the session has recorded, with the run it's from.
    pub(super) recorded: Option<(RunId, u64)>,
    /// How many waits for the agent to pick up again have begun, so a timer ends only its own.
    pub(super) pickup_waits: u64,
    pub(super) agent_opened: bool,
    pub(super) agent_session_id: Option<AgentSessionId>,
    pub(super) capabilities: Option<protocol::model::RunCapabilities>,
    /// What the next agent launches with.
    pub(super) settings: SessionSettings,
    pub(super) closed_at_ms: Option<i64>,
    /// Only exists while someone is subscribed. It's a large buffer, and most sessions a long-running
    /// hub has touched have nobody watching.
    pub(super) live: Option<broadcast::Sender<Live>>,
}

pub(crate) struct LiveSession {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub(super) agent: AgentKind,
    /// So a failed setup is mentioned once, rather than on every prompt.
    pub(super) warned_about_setup: std::sync::atomic::AtomicBool,
    pub(super) hub: Weak<Hub>,
    pub(super) commands: mpsc::Sender<Command>,
    pub(super) ingest: mpsc::Sender<Item>,
    pub(super) state: Mutex<LiveState>,
    /// Held while the session's record changes and is announced, by an event being recorded or by filing which account
    /// a launch runs under, so they happen one at a time and the feed hears each copy in the order they were made.
    pub(super) recording: tokio::sync::Mutex<()>,
    /// Held while a queued prompt is sent, and while the queue changes, so a change can't land between reading the
    /// prompt and taking it out of the queue, and be lost.
    pub(super) queue_lock: tokio::sync::Mutex<()>,
    /// Held while the model, mode or effort is being changed, and while an agent is being started with them,
    /// so a change can't fall between the two and reach neither, and to keep an agent from starting meanwhile.
    pub(super) configuring: tokio::sync::Mutex<()>,
}

/// Tells subscribers something changed, under the lock, so their order matches `version`.
pub(super) fn publish(state: &mut LiveState, item: LiveItem) {
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
        open_approvals: &[ApprovalId],
        carried: Option<CarriedOver>,
    ) -> Arc<Self> {
        let has_queue = !stored.session.queued.is_empty();
        let (agent, recorded, untaken) = match carried {
            Some(CarriedOver { link, run, recorded, untaken }) => (Some(AgentRun { link, run }), recorded, untaken),
            None => (None, None, Vec::new()),
        };
        // The agent still running is the one that asked whatever is open, and runs on what it runs in the background.
        let asked_by = agent.map(|agent| agent.run);
        // What's in the background is as the record lists it: the agent's, if it carries on with one, or else only a
        // wait, which outlives it.
        let progress = Progress::recorded(&stored.session, open_approvals, asked_by, untaken);
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (ingest_tx, ingest_rx) = mpsc::channel(1024);
        let session = Arc::new(LiveSession {
            id: stored.session.id.clone(),
            workspace_id: stored.session.workspace_id.clone(),
            agent: stored.session.agent,
            warned_about_setup: std::sync::atomic::AtomicBool::new(false),
            hub: Arc::downgrade(hub),
            commands: commands_tx,
            ingest: ingest_tx,
            state: Mutex::new(LiveState {
                last_seq: stored.session.last_seq,
                progress,
                tail: Tail::default(),
                version: 0,
                context: stored.session.context.clone(),
                prompt_cache: stored.session.prompt_cache.clone(),
                hearing: None,
                agent,
                recorded,
                pickup_waits: 0,
                agent_opened: stored.agent_opened,
                agent_session_id: stored.agent_session_id,
                capabilities: stored.session.agent_capabilities.clone(),
                settings: stored.session.settings(),
                closed_at_ms: stored.session.closed_at_ms,
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
            // The last hub's wait for the agent to pick up again went with it.
            if session.state.lock().progress.waits_for_pickup() {
                session.wait_for_pickup();
            }
        }
        // A queue the last hub left ready to send.
        if has_queue && session.ready_for_queue() {
            session.wake_queue();
        }
        session
    }

    pub(super) fn hub(&self) -> Result<Arc<Hub>, ApiError> {
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
        self.state.lock().progress.status
    }

    /// The kind of agent it runs, as its record names it, which never changes.
    pub(crate) fn agent(&self) -> AgentKind {
        self.agent
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
        let settings = self.state.lock().settings.clone().changed(options.clone());
        hub.offered(self.agent, &settings.into())?;
        let mut agent_took_it = false;
        if let Some(link) = hub.existing_worker(&self.workspace_id) {
            let delivery = match self.askable(&link)? {
                Askable::Claimed(claims) => {
                    let settings = options
                        .for_capabilities(&claims)
                        .map_err(|e| ApiError::new(ErrorCode::Unsupported, e.to_string()))?;
                    self.control_claimed(&link, protocol::capability::Control::Change(settings), &claims).await?
                }
                Askable::Unclaimed => self.configure_previous(&link, options.clone()).await?,
                Askable::NotRunning => Delivery::AgentNotRunning,
            };
            agent_took_it = delivery == Delivery::Delivered;
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
    pub(super) async fn keep_options(&self, hub: &Hub, change: SessionSettingsChange) -> Result<(), ApiError> {
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
            hub.let_go_of_bytes(&FileFor::Session(self.id.clone()), &let_go_of).await;
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

    pub(super) fn is_closed(&self) -> bool {
        !self.state.lock().is_open()
    }
}

impl LiveState {
    /// Whether the chat is open, as [`Session::is_open`] says.
    pub(super) fn is_open(&self) -> bool {
        self.closed_at_ms.is_none()
    }
}
