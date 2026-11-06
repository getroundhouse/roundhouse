//! One session, while the hub has it in memory.
//!
//! Two tasks do the work. `ingest` records what the agent said, and `commands` carries out what
//! clients ask for, which means waiting on worker replies. They're separate because those replies
//! arrive on the same stream as agent output: if one task did both, waiting for a reply could stop
//! the output being read, and the reply behind it would never arrive.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{
    AgentRunState, ApprovalDecision, ApprovalOutcome, EventBody, ExitReason, NoticeLevel, SessionEvent,
};
use protocol::model::{AgentKind, Seq, SessionId, SessionStatus, WorkspaceId, WorkspaceStatus};
use protocol::tail::{StreamDelta, Tail};
use protocol::worker::{AgentSpec, WorkerRequest, WorkerResponse};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{error, warn};

use crate::Hub;
use crate::store::{StoredSession, cancelled_approval};
use crate::worker_link::WorkerLink;

const LIVE_CAPACITY: usize = 4096;

/// What arrives from a worker.
pub enum Item {
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

pub enum Command {
    Prompt { text: String, client_msg_id: Option<String>, reply: oneshot::Sender<Result<(Seq, bool), ApiError>> },
    Approve { approval_id: String, decision: ApprovalDecision, reply: oneshot::Sender<Result<(), ApiError>> },
    Cancel { reply: oneshot::Sender<Result<(), ApiError>> },
    Stop { reply: oneshot::Sender<Result<(), ApiError>> },
}

#[derive(Clone, Debug)]
pub enum LiveItem {
    /// The log now reaches this `seq`.
    Appended(Seq),
    Delta(StreamDelta),
}

/// `version` puts every change in one order, whichever task made it, so a subscriber can tell what
/// a snapshot already includes.
#[derive(Clone, Debug)]
pub struct Live {
    pub version: u64,
    pub item: LiveItem,
}

pub struct Snapshot {
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
    /// Only exists while someone is subscribed. It's a large buffer, and most sessions a long-running
    /// hub has touched have nobody watching.
    live: Option<broadcast::Sender<Live>>,
}

pub struct LiveSession {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    agent: AgentKind,
    agent_session_id: String,
    model: Option<String>,
    permission_mode: Option<String>,
    hub: Weak<Hub>,
    commands: mpsc::Sender<Command>,
    ingest: mpsc::Sender<Item>,
    state: Mutex<LiveState>,
    recording: tokio::sync::Mutex<()>,
}

/// The status after an event. A prompt sent mid-turn folds into that turn and produces no result of
/// its own, so `AgentState` is what says when the agent is really idle. `TurnEnded` also goes to
/// idle, in case a future CLI stops sending it, and any state event after that corrects it.
pub fn next_status(current: SessionStatus, body: &EventBody, approvals_left: usize) -> SessionStatus {
    match body {
        EventBody::UserPrompt { .. } => SessionStatus::Running,
        EventBody::AgentState { state: AgentRunState::Running } => {
            if approvals_left > 0 {
                SessionStatus::WaitingApproval
            } else {
                SessionStatus::Running
            }
        }
        EventBody::AgentState { state: AgentRunState::RequiresAction } => SessionStatus::WaitingApproval,
        EventBody::AgentState { state: AgentRunState::Idle } => SessionStatus::Idle,
        EventBody::ApprovalRequested { .. } => SessionStatus::WaitingApproval,
        EventBody::ApprovalResolved { .. } if current == SessionStatus::WaitingApproval && approvals_left == 0 => {
            SessionStatus::Running
        }
        EventBody::TurnEnded { .. } => SessionStatus::Idle,
        EventBody::AgentExited { .. } if matches!(current, SessionStatus::Running | SessionStatus::WaitingApproval) => {
            SessionStatus::Interrupted
        }
        _ => current,
    }
}

/// For events the hub records itself, which belong to no run of an agent.
const NO_RUN: u64 = 0;

/// How many approvals stay open once this event is applied.
fn approvals_after(pending: &HashMap<String, u64>, body: &EventBody) -> usize {
    match body {
        EventBody::ApprovalRequested { approval_id, .. } if !pending.contains_key(approval_id) => pending.len() + 1,
        EventBody::ApprovalResolved { approval_id, .. } if pending.contains_key(approval_id) => pending.len() - 1,
        _ => pending.len(),
    }
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
    pub fn start(hub: &Arc<Hub>, stored: StoredSession, open_approvals: Vec<String>) -> Arc<Self> {
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (ingest_tx, ingest_rx) = mpsc::channel(1024);
        let session = Arc::new(LiveSession {
            id: stored.session.id.clone(),
            workspace_id: stored.session.workspace_id.clone(),
            agent: stored.session.agent,
            agent_session_id: stored.agent_session_id,
            model: stored.session.model.clone(),
            permission_mode: stored.session.permission_mode.clone(),
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
                live: None,
            }),
            recording: tokio::sync::Mutex::new(()),
        });
        tokio::spawn(run_commands(session.clone(), commands_rx));
        tokio::spawn(run_ingest(session.clone(), ingest_rx));
        session
    }

    fn hub(&self) -> Result<Arc<Hub>, ApiError> {
        self.hub.upgrade().ok_or_else(|| ApiError::new(ErrorCode::Internal, "the hub is shutting down"))
    }

    pub async fn command(&self, command: Command) -> Result<(), ApiError> {
        self.commands.send(command).await.map_err(|_| ApiError::new(ErrorCode::Internal, "the session has stopped"))
    }

    pub async fn send_item(&self, item: Item) {
        if self.ingest.send(item).await.is_err() {
            warn!(session = self.id, "dropped worker output for a stopped session");
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Live> {
        let mut state = self.state.lock().unwrap();
        state.live.get_or_insert_with(|| broadcast::channel(LIVE_CAPACITY).0).subscribe()
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        Snapshot { last_seq: state.last_seq, tail: state.tail.clone(), version: state.version }
    }

    pub fn status(&self) -> SessionStatus {
        self.state.lock().unwrap().status
    }

    pub fn agent_running(&self) -> bool {
        self.state.lock().unwrap().agent.is_some()
    }

    /// Commits to the log first, so nothing is seen that isn't durable, then updates live state.
    /// One at a time: both tasks record, and status has to change in the same order as the log.
    /// `speaks_for_session` is false for a leftover event from a replaced agent, which mustn't change
    /// the status of the session its replacement is now running.
    async fn record(
        &self,
        body: EventBody,
        raw: Option<String>,
        client_msg_id: Option<String>,
        speaks_for_session: bool,
        run: u64,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let _in_order = self.recording.lock().await;
        let hub = self.hub()?;
        let (next, new_status) = {
            let state = self.state.lock().unwrap();
            if speaks_for_session {
                let next = next_status(state.status, &body, approvals_after(&state.pending_approvals, &body));
                (next, (next != state.status).then_some(next))
            } else {
                (state.status, None)
            }
        };
        let id = self.id.clone();
        let for_store = body.clone();
        let appended = hub
            .blocking(move |store| store.append(&id, &for_store, raw.as_deref(), client_msg_id.as_deref(), new_status))
            .await?;
        if appended.duplicate {
            return Ok((appended.event, true));
        }
        let seq = appended.event.seq;
        let mut state = self.state.lock().unwrap();
        state.last_seq = seq;
        state.tail.on_event(&body);
        match &body {
            EventBody::ApprovalRequested { approval_id, .. } => {
                state.pending_approvals.insert(approval_id.clone(), run);
            }
            EventBody::ApprovalResolved { approval_id, .. } => {
                state.pending_approvals.remove(approval_id);
            }
            _ => {}
        }
        state.status = next;
        publish(&mut state, LiveItem::Appended(seq));
        drop(state);
        Ok((appended.event, false))
    }

    /// Records something the hub itself knows, as opposed to agent output.
    async fn record_own(&self, body: EventBody) -> Result<(SessionEvent, bool), ApiError> {
        self.record(body, None, None, true, NO_RUN).await
    }

    fn apply_delta(&self, delta: StreamDelta) {
        let mut state = self.state.lock().unwrap();
        state.tail.apply(&delta);
        publish(&mut state, LiveItem::Delta(delta));
    }

    /// Closes open approvals that nobody can answer now: those from `run`, or all if it's `None`.
    async fn cancel_approvals(&self, run: Option<u64>) {
        let open: Vec<String> = {
            let state = self.state.lock().unwrap();
            let from_this_run = |asked_by: u64| run.is_none_or(|run| asked_by == run || asked_by == NO_RUN);
            state.pending_approvals.iter().filter(|(_, by)| from_this_run(**by)).map(|(id, _)| id.clone()).collect()
        };
        for approval_id in open {
            self.record_or_log(cancelled_approval(approval_id)).await;
        }
    }

    async fn record_or_log(&self, body: EventBody) {
        if let Err(e) = self.record_own(body).await {
            error!(session = self.id, "could not record an event: {e}");
        }
    }

    /// Reports that the agent is gone, unless a newer one has already replaced it.
    async fn agent_lost(&self, link: u64, reason: ExitReason) {
        let was_running = {
            let mut state = self.state.lock().unwrap();
            let matches = state.agent.is_some_and(|agent| agent.link == link);
            if matches {
                state.agent = None;
            }
            matches
        };
        if was_running {
            self.cancel_approvals(None).await;
            self.record_or_log(EventBody::AgentExited { reason, code: None, signal: None, stderr_tail: vec![] }).await;
        }
    }

    /// Finds or starts the worker and the agent, and returns the connection to use.
    async fn ensure_agent(&self) -> Result<Arc<WorkerLink>, ApiError> {
        let hub = self.hub()?;
        let link = hub.ensure_worker(&self.workspace_id).await?;
        let known = self.state.lock().unwrap().agent;
        if known.is_some_and(|agent| agent.link == link.id) {
            return Ok(link);
        }
        if let Some(previous) = known {
            // The worker was replaced before its loss reached this session.
            self.agent_lost(previous.link, ExitReason::WorkerLost).await;
        }
        let workspace = hub.workspace(&self.workspace_id).await?;
        let cwd =
            workspace.path.ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the workspace has no worktree yet"))?;
        let resume = self.state.lock().unwrap().agent_opened;
        let spec = AgentSpec {
            session_id: self.id.clone(),
            run: hub.next_run(),
            agent: self.agent,
            cwd,
            agent_session_id: self.agent_session_id.clone(),
            resume,
            model: self.model.clone(),
            permission_mode: self.permission_mode.clone(),
        };
        match link.call(WorkerRequest::AgentStart(spec)).await? {
            WorkerResponse::AgentStarted { pid, run, already_running } => {
                self.state.lock().unwrap().agent = Some(AgentRun { link: link.id, run });
                if !already_running {
                    // Whatever is still open was asked by a process that no longer exists.
                    self.cancel_approvals(None).await;
                    self.record_or_log(EventBody::AgentStarted { pid, resumed: resume }).await;
                }
                Ok(link)
            }
            other => Err(ApiError::new(ErrorCode::Worker, format!("unexpected reply: {other:?}"))),
        }
    }

    async fn forward_prompt(&self, text: String) -> Result<(), ApiError> {
        // Twice, because the agent can exit between checking on it and sending to it.
        for _ in 0..2 {
            let link = self.ensure_agent().await?;
            let request = WorkerRequest::AgentPrompt { session_id: self.id.clone(), text: text.clone() };
            match link.call(request).await? {
                WorkerResponse::AgentNotRunning => {
                    let mut state = self.state.lock().unwrap();
                    if state.agent.is_some_and(|agent| agent.link == link.id) {
                        state.agent = None;
                    }
                }
                _ => return Ok(()),
            }
        }
        Err(ApiError::new(ErrorCode::Worker, "the agent keeps exiting before it can take the prompt"))
    }

    async fn handle_prompt(
        &self,
        text: String,
        client_msg_id: Option<String>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    ) {
        let hub = match self.hub() {
            Ok(hub) => hub,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        match hub.workspace(&self.workspace_id).await {
            Ok(workspace) if workspace.status == WorkspaceStatus::Ready => {}
            Ok(_) => {
                let _ = reply.send(Err(ApiError::new(ErrorCode::Conflict, "the workspace isn't ready")));
                return;
            }
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        }
        let body = EventBody::UserPrompt { text: text.clone(), client_msg_id: client_msg_id.clone() };
        let (event, duplicate) = match self.record(body, None, client_msg_id, true, NO_RUN).await {
            Ok(recorded) => recorded,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        // Recorded, so the prompt is real and the client can stop retrying. From here, failures are
        // reported in the log, where every client can see them.
        let _ = reply.send(Ok((event.seq, duplicate)));
        if duplicate {
            return;
        }
        if let Err(e) = self.forward_prompt(text).await {
            warn!(session = self.id, "could not deliver a prompt: {e}");
            self.cancel_approvals(None).await;
            self.record_or_log(EventBody::AgentExited {
                reason: ExitReason::StartFailed,
                code: None,
                signal: None,
                stderr_tail: vec![e.message.clone()],
            })
            .await;
        }
    }

    async fn handle_approve(&self, approval_id: String, decision: ApprovalDecision) -> Result<(), ApiError> {
        {
            let state = self.state.lock().unwrap();
            let Some(asked_by) = state.pending_approvals.get(&approval_id) else {
                return Err(ApiError::new(
                    ErrorCode::Conflict,
                    format!("approval {approval_id} isn't waiting for an answer"),
                ));
            };
            if state.agent.map(|agent| agent.run) != Some(*asked_by) {
                return Err(ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"));
            }
        }
        let outcome = match decision {
            ApprovalDecision::Allow { .. } => ApprovalOutcome::Allowed,
            ApprovalDecision::Deny { .. } => ApprovalOutcome::Denied,
        };
        self.record_own(EventBody::ApprovalResolved { approval_id: approval_id.clone(), outcome }).await?;
        let hub = self.hub()?;
        let link = hub
            .existing_worker(&self.workspace_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"))?;
        let request = WorkerRequest::AgentApproval { session_id: self.id.clone(), approval_id, decision };
        match link.call(request).await? {
            WorkerResponse::AgentNotRunning => {
                self.record_or_log(EventBody::Notice {
                    level: NoticeLevel::Warning,
                    message: "The agent had already exited when this approval was answered.".to_string(),
                })
                .await;
                Err(ApiError::new(ErrorCode::Conflict, "the agent has exited"))
            }
            _ => Ok(()),
        }
    }

    /// Only touches a worker that's already up. There's nothing to cancel without one.
    async fn handle_cancel(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        link.call(WorkerRequest::AgentCancel { session_id: self.id.clone() }).await.map(|_| ())
    }

    async fn handle_stop(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        link.call(WorkerRequest::AgentStop { session_id: self.id.clone() }).await.map(|_| ())
    }
}

async fn run_commands(session: Arc<LiveSession>, mut commands: mpsc::Receiver<Command>) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Prompt { text, client_msg_id, reply } => session.handle_prompt(text, client_msg_id, reply).await,
            Command::Approve { approval_id, decision, reply } => {
                let _ = reply.send(session.handle_approve(approval_id, decision).await);
            }
            Command::Cancel { reply } => {
                let _ = reply.send(session.handle_cancel().await);
            }
            Command::Stop { reply } => {
                let _ = reply.send(session.handle_stop().await);
            }
        }
    }
}

async fn run_ingest(session: Arc<LiveSession>, mut items: mpsc::Receiver<Item>) {
    while let Some(item) = items.recv().await {
        match item {
            Item::Event { run, body, raw } => ingest_event(&session, run, body, raw).await,
            Item::Delta(delta) => session.apply_delta(delta),
            Item::WorkerLost(link) => session.agent_lost(link, ExitReason::WorkerLost).await,
            Item::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}

async fn ingest_event(session: &Arc<LiveSession>, run: u64, body: EventBody, raw: Option<String>) {
    let current = session.state.lock().unwrap().agent.is_some_and(|agent| agent.run == run);
    match &body {
        EventBody::AgentInit { .. } => mark_opened(session).await,
        EventBody::AgentExited { .. } => {
            if current {
                session.state.lock().unwrap().agent = None;
            }
            session.cancel_approvals(Some(run)).await;
        }
        _ => {}
    }
    let stale_approval = match &body {
        EventBody::ApprovalRequested { approval_id, .. } if !current => Some(approval_id.clone()),
        _ => None,
    };
    if let Err(e) = session.record(body, raw, None, current, run).await {
        error!(session = session.id, "could not record agent output: {e}");
    }
    if let Some(approval_id) = stale_approval {
        session.record_or_log(cancelled_approval(approval_id)).await;
    }
}

async fn mark_opened(session: &Arc<LiveSession>) {
    let first_time = {
        let mut state = session.state.lock().unwrap();
        !std::mem::replace(&mut state.agent_opened, true)
    };
    if !first_time {
        return;
    }
    let Ok(hub) = session.hub() else { return };
    let id = session.id.clone();
    if let Err(e) = hub.blocking(move |store| store.set_agent_opened(&id)).await {
        error!(session = session.id, "could not record that the agent opened: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::event::TurnOutcome;

    fn turn_ended() -> EventBody {
        EventBody::TurnEnded {
            outcome: TurnOutcome::Success,
            result: None,
            errors: vec![],
            duration_ms: None,
            cost_usd: None,
            num_turns: None,
        }
    }

    fn state(state: AgentRunState) -> EventBody {
        EventBody::AgentState { state }
    }

    fn exited() -> EventBody {
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] }
    }

    fn resolved() -> EventBody {
        EventBody::ApprovalResolved { approval_id: "a".into(), outcome: ApprovalOutcome::Allowed }
    }

    use SessionStatus::*;

    #[test]
    fn a_prompt_starts_running_from_any_state() {
        let prompt = EventBody::UserPrompt { text: "x".into(), client_msg_id: None };
        for from in [Idle, Running, WaitingApproval, Interrupted] {
            assert_eq!(next_status(from, &prompt, 0), Running);
        }
    }

    #[test]
    fn the_agents_own_state_decides_idle() {
        assert_eq!(next_status(Running, &state(AgentRunState::Idle), 0), Idle);
        assert_eq!(next_status(Running, &state(AgentRunState::RequiresAction), 0), WaitingApproval);
        assert_eq!(next_status(Idle, &state(AgentRunState::Running), 0), Running);
    }

    #[test]
    fn turn_end_still_goes_idle_if_the_agent_sends_no_state_events() {
        assert_eq!(next_status(Running, &turn_ended(), 0), Idle);
    }

    #[test]
    fn waiting_only_ends_when_the_last_approval_is_answered() {
        assert_eq!(next_status(WaitingApproval, &resolved(), 1), WaitingApproval);
        assert_eq!(next_status(WaitingApproval, &resolved(), 0), Running);
        // A running state event doesn't hide an approval that's still open.
        assert_eq!(next_status(WaitingApproval, &state(AgentRunState::Running), 1), WaitingApproval);
    }

    #[test]
    fn losing_the_agent_mid_turn_is_an_interruption_but_losing_it_when_idle_is_nothing() {
        assert_eq!(next_status(Running, &exited(), 0), Interrupted);
        assert_eq!(next_status(WaitingApproval, &exited(), 0), Interrupted);
        assert_eq!(next_status(Idle, &exited(), 0), Idle);
        assert_eq!(next_status(Interrupted, &exited(), 0), Interrupted);
    }
}
