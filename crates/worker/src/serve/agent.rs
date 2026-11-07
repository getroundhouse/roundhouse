//! Starting and stopping agents, and forwarding what they say to the hub.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use agents::{AgentOptions, AgentOutput, BinaryLookup};
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::SessionId;
use protocol::worker::{AgentSpec, WorkerFrame, WorkerResponse};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::info;

use super::{RunningAgent, State};

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
    if let Some(agent) = state.agents.lock().get(&session_id).filter(|a| !a.process.is_gone()) {
        return Ok(WorkerResponse::AgentStarted { pid: agent.process.pid(), run: agent.run, already_running: true });
    }
    let (tx, rx) = mpsc::channel(1024);
    let options = AgentOptions {
        binary: BinaryLookup {
            explicit: state.config.claude_bin.clone(),
            path: state.config.path.clone(),
            home: state.config.home.clone(),
        },
        // Named by run, because a replaced agent removes its own file as it finishes, which can be
        // after its replacement has started.
        pid_file: state.config.state_dir.as_ref().map(|dir| dir.join("agents").join(format!("{session_id}-{run}.pid"))),
    };
    let process = agents::spawn(spec, tx, options)?;
    let pid = process.pid();
    info!(pid, %session_id, run, "started agent");
    // Replaces an entry whose process has gone. Its forwarder finishes delivering its last output.
    state.agents.lock().insert(session_id.clone(), RunningAgent { process, run });
    forwarders.lock().await.spawn(forward_output(state.clone(), session_id, run, rx));
    Ok(WorkerResponse::AgentStarted { pid, run, already_running: false })
}

async fn forward_output(state: Arc<State>, session_id: SessionId, run: u64, mut output: mpsc::Receiver<AgentOutput>) {
    while let Some(item) = output.recv().await {
        let frame = match item {
            AgentOutput::Event { body, raw } => {
                WorkerFrame::AgentEvent { session_id: session_id.clone(), run, body, raw }
            }
            AgentOutput::Delta(delta) => WorkerFrame::AgentDelta { session_id: session_id.clone(), delta },
            AgentOutput::Exited { code, signal, stderr_tail, requested } => {
                {
                    let mut agents = state.agents.lock();
                    if agents.get(&session_id).is_some_and(|agent| agent.run == run) {
                        agents.remove(&session_id);
                    }
                }
                let reason = if requested { ExitReason::Requested } else { ExitReason::Unexpected };
                let body = EventBody::AgentExited { reason, code, signal, stderr_tail };
                WorkerFrame::AgentEvent { session_id: session_id.clone(), run, body, raw: None }
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
    model: Option<&str>,
    permission_mode: Option<&str>,
) -> Result<WorkerResponse, ApiError> {
    let Some(agent) = state.live_agent(session_id) else {
        return Ok(WorkerResponse::AgentNotRunning);
    };
    match agent.configure(model, permission_mode).await {
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
