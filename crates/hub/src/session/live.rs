use super::ChatSettings;
use super::had_servers::HadServers;
use super::reloading::Reloading;
use super::{Starting, commands, ingest, limits, messages, progress};
use crate::attachment_files::FileFor;
use crate::error::internal;
use crate::hub::Hub;
use crate::store;
use crate::store::SessionLogin;
use crate::tool_servers::ServersAsSent;
use crate::worker_link::WorkerLinkId;
use commands::{Sendable, run_commands};
use ingest::run_ingest;
use messages::{Command, Item};
use parking_lot::Mutex;
use progress::Progress;
use protocol::client::ApiError;
use protocol::model::{
    AgentKind, AgentSessionId, RunId, Seq, Session, SessionId, SessionStatus, SubagentId, WorkspaceId,
};
use protocol::tail::{StreamDelta, Tail};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tracing::{Instrument as _, warn};

const LIVE_CAPACITY: usize = 4096;

#[derive(Clone, Debug)]
pub(crate) enum LiveItem {
    /// The log now reaches `seq`, with an event from the subagent `subagent`, or the agent's own.
    Appended {
        seq: Seq,
        subagent: Option<SubagentId>,
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

/// One launch of an agent, on one connection to a worker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct AgentRun {
    pub(super) link: WorkerLinkId,
    pub(super) run: RunId,
}

/// What a session carries on with from an earlier hub, whose agent is still running on a worker this hub has
/// connected to again.
#[derive(Clone)]
pub(crate) struct CarriedOver {
    /// The connection this hub reached the agent's worker on.
    pub link: WorkerLinkId,
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
    /// What the API's cache holds of the conversation, as the session last heard, which starts as the hub last told it.
    pub(super) prompt_cache: super::cache::CacheHeld,
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
    pub(super) settings: ChatSettings,
    /// Which login the chat is on, as its record says, which the next agent launches under. On none until a launch is
    /// handed a login, which puts it on the hub's for that account, or its worker says whose the ambient login an agent
    /// of it runs under is, which puts it on that.
    pub(super) login: SessionLogin,
    pub(super) closed_at_ms: Option<i64>,
    /// How the person talks with the agent, as the chat's record says, with the device that has the conversation while
    /// conversation mode is on.
    pub(super) interaction: protocol::model::Interaction,
    /// The reply that device is reading out, if any.
    pub(super) reading_out: super::reading_out::ReadingOut,
    /// How the run has had each of its tool servers, as [`super::reconnecting`] says.
    pub(super) had_servers: Option<HadServers>,
    /// What the run has been asked to connect to again, as [`super::reloading`] says.
    pub(super) reloading: Option<Reloading>,
    /// How the run was last told the person talks with the agent, as [`super::mode_told`] says.
    pub(super) mode_told: Option<(RunId, protocol::model::InteractionMode)>,
    /// Prompts recorded for the agent that it's sent once its turn ends, oldest first, as
    /// [`super::commands::Sendable`]'s module says. Only this hub has them, so a hub that goes before the turn ends
    /// loses them, and they stay untaken.
    pub(super) after_the_turn: Vec<Sendable>,
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
    /// The hub's [`crate::Timers::picks_up_within`], copied as the session starts, since it holds the hub weakly.
    pub(super) picks_up_within: Duration,
    pub(super) commands: mpsc::Sender<Command>,
    pub(super) ingest: mpsc::Sender<Item>,
    pub(super) state: Mutex<LiveState>,
    /// Held while the session's record changes and is announced, by an event being recorded or by filing which account
    /// a launch runs under, so they happen one at a time and the feed hears each copy in the order they were made. A
    /// check of the live state that has to agree with what a client can read is made under it too, since the store has
    /// each event before the live state takes it in.
    pub(super) recording: tokio::sync::Mutex<()>,
    /// Held while a queued prompt is sent, and while the queue changes, so a change can't land between reading the
    /// prompt and taking it out of the queue, and be lost.
    pub(super) queue_lock: tokio::sync::Mutex<()>,
    /// Held while the model, mode or effort is being changed, and while an agent is being started with them,
    /// so a change can't fall between the two and reach neither, and to keep an agent from starting meanwhile.
    pub(super) configuring: tokio::sync::Mutex<()>,
    /// Held while the reply being read out changes, or one is settled, so a place can't land after its reply's end.
    pub(super) settling: tokio::sync::Mutex<()>,
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
    /// From what it starts from, as [`Starting::read`] reads it. `carried` is what it carries on with from an earlier
    /// hub, if its agent is still running.
    pub(crate) fn start(hub: &Arc<Hub>, starting: Starting, carried: Option<CarriedOver>) -> Arc<Self> {
        let Starting { stored, open_approvals, failing } = starting;
        let has_queue = !stored.session.queued.is_empty();
        let (agent, recorded, untaken) = match carried {
            Some(CarriedOver { link, run, recorded, untaken }) => (Some(AgentRun { link, run }), recorded, untaken),
            None => (None, None, Vec::new()),
        };
        // The agent still running is the one that asked whatever is open, and runs on what it runs in the background.
        let asked_by = agent.map(|agent| agent.run);
        // What's in the background is as the record lists it: the agent's, if it carries on with one, or else only a
        // wait, which outlives it.
        let progress = Progress::recorded(&stored.session, &open_approvals, asked_by, untaken, failing);
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (ingest_tx, ingest_rx) = mpsc::channel(1024);
        let session = Arc::new(LiveSession {
            id: stored.session.id.clone(),
            workspace_id: stored.session.workspace_id.clone(),
            agent: stored.session.agent,
            warned_about_setup: std::sync::atomic::AtomicBool::new(false),
            hub: Arc::downgrade(hub),
            picks_up_within: hub.timers.picks_up_within,
            commands: commands_tx,
            ingest: ingest_tx,
            state: Mutex::new(LiveState {
                last_seq: stored.session.last_seq,
                progress,
                tail: Tail::default(),
                version: 0,
                prompt_cache: super::cache::CacheHeld::told_of(hub, &stored.session),
                hearing: None,
                agent,
                recorded,
                pickup_waits: 0,
                agent_opened: stored.agent_opened,
                agent_session_id: stored.agent_session_id,
                capabilities: stored.session.agent_capabilities.clone(),
                settings: ChatSettings::of(&stored.session),
                login: SessionLogin::of_record(&stored.session),
                closed_at_ms: stored.session.closed_at_ms,
                interaction: stored.session.interaction.clone(),
                reading_out: super::reading_out::ReadingOut::default(),
                had_servers: agent.map(|agent| HadServers::new(agent.run, ServersAsSent::default())),
                reloading: None,
                mode_told: None,
                after_the_turn: Vec::new(),
                live: None,
            }),
            recording: tokio::sync::Mutex::new(()),
            queue_lock: tokio::sync::Mutex::new(()),
            configuring: tokio::sync::Mutex::new(()),
            settling: tokio::sync::Mutex::new(()),
        });
        tokio::spawn(run_commands(session.clone(), commands_rx).instrument(session.span()));
        tokio::spawn(run_ingest(session.clone(), ingest_rx).instrument(session.span()));
        if let Some(agent) = agent {
            session.carried_on(agent.run);
        }
        // A queue the last hub left ready to send.
        if has_queue && session.ready_for_queue() {
            session.wake_queue();
        }
        session
    }

    /// Has a session that was made before its agent's worker dialed the hub carry on with `carried`, as one made after
    /// does, as a workspace whose worker dialed late has each of its sessions do. A client could have followed the
    /// session meanwhile, which makes it, with no agent, and what it recorded since is what the agent finds. The
    /// `capabilities` are what the session's record says its run claims now. Does
    /// nothing to one that has an agent already.
    pub(crate) fn carry_on_with(
        self: &Arc<Self>,
        carried: CarriedOver,
        capabilities: Option<protocol::model::RunCapabilities>,
    ) {
        let CarriedOver { link, run, recorded, untaken } = carried;
        {
            let mut state = self.state.lock();
            if state.agent.is_some() {
                return;
            }
            state.agent = Some(AgentRun { link, run });
            state.capabilities = capabilities;
            state.recorded = recorded;
            state.had_servers = Some(HadServers::new(run, ServersAsSent::default()));
            for asked_by in state.progress.approvals.values_mut() {
                asked_by.get_or_insert(run);
            }
            state.progress.queue.set_untaken(untaken);
        }
        self.carried_on(run);
    }

    /// Does for `run`, which an earlier hub left running, what the last hub may not have.
    fn carried_on(self: &Arc<Self>, run: RunId) {
        // So what the agent hears of its account's limits is filed under the account, as for one this hub launched.
        self.launched(run);
        // A tool server may have been signed in to or edited as the last hub went.
        self.ask_for_reconnects();
        // The last hub's wait for the agent to pick up again went with it.
        if self.state.lock().progress.waits_for_pickup() {
            self.wait_for_pickup();
        }
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
            warn!(name: "session.output_dropped", session_id = %self.id, "Dropped worker output for a stopped session");
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Live> {
        let mut state = self.state.lock();
        state.live.get_or_insert_with(|| broadcast::channel(LIVE_CAPACITY).0).subscribe()
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let state = self.state.lock();
        // What's read out is the device's, rather than the agent's, so it's kept apart from what the agent writes.
        let tail = Tail { speaking: state.reading_out.speaking(), ..state.tail.clone() };
        Snapshot { last_seq: state.last_seq, tail, version: state.version }
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

    pub(crate) async fn close(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let closed_at = store::now_ms();
        let id = self.id.clone();
        {
            // An agent being started right now either finishes first, and is stopped below, or sees this.
            let starts = self.configuring.lock().await;
            let let_go_of = hub.blocking(move |store| store.set_session_closed(&id, Some(closed_at))).await?;
            self.state.lock().closed_at_ms = Some(closed_at);
            // Closing it ended its waits, whose records keep what they last read from now.
            hub.wait_reads_settled().await;
            hub.let_go_of_bytes(&FileFor::Session(self.id.clone()), &let_go_of).await;
            // Closing it ended its waits. It's closed whether or not they come off its list.
            if let Err(e) = self.unlist_waits_holding(&starts).await {
                warn!(
                    name: "session.waits_not_cleared",
                    session_id = %self.id,
                    error = %e,
                    "Couldn't take a closed chat's waits off its list"
                );
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
