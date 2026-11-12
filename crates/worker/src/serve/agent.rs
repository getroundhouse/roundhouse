//! Starting and stopping agents, forwarding what they say to the hub, and the one-off calls that title a prompt and
//! read the agent's account.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use agents::{AgentOptions, AgentOutput, BinaryLookup};
use protocol::attachment::NewAttachment;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{AgentKind, RunId, SessionId, SessionSettingsChange};
use protocol::worker::{AgentSpec, WorkerFrame, WorkerResponse};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::info;

use super::{RunningAgent, State};

/// Gives up once the worker is stopping, which ends the call, so a title nobody will see can't keep it running.
pub(crate) async fn title_for(
    state: &State,
    agent: AgentKind,
    prompt: &str,
    images: &[NewAttachment],
) -> Result<WorkerResponse, ApiError> {
    let options = one_off(state, "title");
    let title = agents::title_for(agent, prompt, images, &options, &state.config.scratch_dir);
    let title = until_stopping(state, title).await?;
    Ok(WorkerResponse::Title { title: title?.to_string() })
}

/// What a one-off call to an agent's CLI runs with: a record of its own, numbered, so the provider can stop the call
/// if this worker is killed.
fn one_off(state: &State, what: &str) -> AgentOptions {
    let call = state.calls.fetch_add(1, Ordering::Relaxed);
    let record = |dir: &std::path::PathBuf| dir.join("agents").join(format!("{what}-{call}.pid"));
    AgentOptions { binary: binary(state), pid_file: state.config.state_dir.as_ref().map(record) }
}

/// What `call` answers, unless the worker starts stopping first, which drops the call and so stops what it started.
/// Nobody is left to hear the answer then, and waiting for it would hold the worker up.
async fn until_stopping<T>(state: &State, call: impl Future<Output = T>) -> Result<T, ApiError> {
    let mut stopping = state.stopping.subscribe();
    tokio::select! {
        answer = call => Ok(answer),
        _ = stopping.wait_for(|stopping| *stopping) => {
            Err(ApiError::new(ErrorCode::Worker, "the worker stopped before the agent's CLI answered"))
        }
    }
}

/// The account the agent's CLI is logged in to here.
pub(crate) async fn account_for(state: &State, agent: AgentKind) -> Result<WorkerResponse, ApiError> {
    let options = one_off(state, "account");
    let account = agents::account(agent, &options, &state.config.scratch_dir);
    Ok(WorkerResponse::Account(until_stopping(state, account).await??))
}

pub(crate) async fn start_agent(
    state: &Arc<State>,
    forwarders: &Arc<tokio::sync::Mutex<JoinSet<()>>>,
    spec: AgentSpec,
) -> Result<WorkerResponse, ApiError> {
    let _guard = state.start_lock.lock().await;
    if state.shutting_down.load(Ordering::SeqCst) {
        return Err(ApiError::new(ErrorCode::Worker, "the worker is shutting down"));
    }
    let session_id = spec.session_id.clone();
    let run = spec.run;
    // The agent works in the worktree, which is where what its text points to is read from.
    let worktree = PathBuf::from(&spec.cwd);
    if let Some(agent) = state.agents.lock().get(&session_id).filter(|a| !a.process.is_gone()) {
        return Ok(WorkerResponse::AgentStarted { pid: agent.process.pid(), run: agent.run, already_running: true });
    }
    if let Some(token) = spec.tools_token.clone() {
        state.tools.admit(token, session_id.clone());
    }
    let (tx, rx) = mpsc::channel(1024);
    let options = AgentOptions {
        binary: binary(state),
        // Named by run, because a replaced agent removes its own file as it finishes, which can be
        // after its replacement has started.
        pid_file: state.config.state_dir.as_ref().map(|dir| dir.join("agents").join(format!("{session_id}-{run}.pid"))),
    };
    let process = agents::spawn(spec, tx, options).await?;
    let pid = process.pid();
    info!(pid, %session_id, %run, "started agent");
    // Replaces an entry whose process has gone. Its forwarder finishes delivering its last output.
    state.agents.lock().insert(session_id.clone(), RunningAgent { process, run });
    forwarders.lock().await.spawn(forward_output(state.clone(), session_id, run, worktree, rx));
    Ok(WorkerResponse::AgentStarted { pid, run, already_running: false })
}

/// Sends on what the agent says, in order, with the images its text points to in `worktree`.
async fn forward_output(
    state: Arc<State>,
    session_id: SessionId,
    run: RunId,
    worktree: PathBuf,
    mut output: mpsc::Receiver<AgentOutput>,
) {
    while let Some(item) = output.recv().await {
        let frame = match item {
            AgentOutput::Event { body, raw, mut attachments } => {
                let (body, linked) = crate::linked_images::keep_in(&worktree, body).await;
                attachments.extend(linked);
                WorkerFrame::AgentEvent { session_id: session_id.clone(), run, body, raw, attachments }
            }
            AgentOutput::Delta(delta) => WorkerFrame::AgentDelta { session_id: session_id.clone(), run, delta },
            AgentOutput::Context(context) => WorkerFrame::AgentContext { session_id: session_id.clone(), run, context },
            AgentOutput::Cache(request) => WorkerFrame::AgentCache { session_id: session_id.clone(), run, request },
            AgentOutput::Limits(limits) => WorkerFrame::AgentLimits { session_id: session_id.clone(), run, limits },
            AgentOutput::Exited { code, signal, stderr_tail, requested } => {
                {
                    let mut agents = state.agents.lock();
                    if agents.get(&session_id).is_some_and(|agent| agent.run == run) {
                        agents.remove(&session_id);
                    }
                }
                let reason = if requested { ExitReason::Requested } else { ExitReason::Unexpected };
                let body = EventBody::AgentExited { reason, code, signal, stderr_tail };
                WorkerFrame::AgentEvent {
                    session_id: session_id.clone(),
                    run,
                    body,
                    raw: None,
                    attachments: Vec::new(),
                }
            }
        };
        if state.frames.send(frame).await.is_err() {
            return;
        }
    }
}

pub(crate) async fn configure_agent(
    state: &State,
    session_id: &SessionId,
    options: &SessionSettingsChange,
) -> Result<WorkerResponse, ApiError> {
    let Some(agent) = state.live_agent(session_id) else {
        return Ok(WorkerResponse::AgentNotRunning);
    };
    match agent.configure(options).await {
        Ok(()) => Ok(WorkerResponse::Ok),
        Err(_) if agent.is_gone() => Ok(WorkerResponse::AgentNotRunning),
        Err(e) => Err(e.into()),
    }
}

pub(crate) async fn stop_agent(state: &State, session_id: &SessionId) -> Result<WorkerResponse, ApiError> {
    // Includes one that's on its way out: this resolves once it has finished.
    let agent = state.agents.lock().get(session_id).map(|agent| agent.process.clone());
    match agent {
        Some(agent) => {
            agent.stop().await;
            Ok(WorkerResponse::Ok)
        }
        None => Ok(WorkerResponse::AgentNotRunning),
    }
}

fn binary(state: &State) -> BinaryLookup {
    BinaryLookup {
        explicit: state.config.claude_bin.clone(),
        path: state.config.path.clone(),
        home: state.config.home.clone(),
    }
}
