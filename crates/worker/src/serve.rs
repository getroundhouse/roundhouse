//! The worker's main loop: one stream in from the hub, one stream out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agents::{AgentOptions, AgentOutput, AgentProcess};
use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::frame;
use protocol::worker::{AgentSpec, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::{git, setup};

/// Enough to see what went wrong, without sending a whole install log across.
const SETUP_LOG_TAIL_BYTES: u64 = 256 * 1024;

pub struct Config {
    pub workspaces_root: PathBuf,
    /// Where to record running agents, so they can be found if this process is killed.
    pub state_dir: Option<PathBuf>,
}

struct RunningAgent {
    process: Arc<dyn AgentProcess>,
    run: u64,
}

struct State {
    config: Config,
    agents: Mutex<HashMap<String, RunningAgent>>,
    /// Serializes starts, so two requests can't launch two processes for one conversation.
    start_lock: tokio::sync::Mutex<()>,
    /// Set under `start_lock` once the hub has gone, so a start still in flight can't add an agent
    /// after the worker has decided what to stop.
    shutting_down: AtomicBool,
    /// The running setup script, so it can be stopped.
    setup_pid: Arc<Mutex<Option<u32>>>,
    frames: mpsc::Sender<WorkerFrame>,
}

impl State {
    /// The session's agent, unless its process has gone. Its entry can outlast the process by a
    /// while, because its last output is still on the way to the hub.
    fn live_agent(&self, session_id: &str) -> Option<Arc<dyn AgentProcess>> {
        let agents = self.agents.lock();
        agents.get(session_id).filter(|agent| !agent.process.is_gone()).map(|agent| agent.process.clone())
    }
}

/// Runs until the hub closes the stream, then stops every agent and flushes what they said. Losing
/// the hub ends the worker on purpose, so agents never run with nobody recording them. Milestone 2
/// changes this, with an outbox that lets remote workers keep going.
pub async fn serve<R, W>(reader: R, writer: W, config: Config) -> anyhow::Result<()>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let (frames, frames_rx) = mpsc::channel::<WorkerFrame>(1024);
    let writer_task = tokio::spawn(write_frames(writer, frames_rx));

    let state = Arc::new(State {
        config,
        agents: Mutex::new(HashMap::new()),
        start_lock: tokio::sync::Mutex::new(()),
        shutting_down: AtomicBool::new(false),
        setup_pid: Arc::new(Mutex::new(None)),
        frames: frames.clone(),
    });
    let forwarders = Arc::new(tokio::sync::Mutex::new(JoinSet::new()));

    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut reader).take(frame::MAX_FRAME_BYTES as u64 + 1).read_until(b'\n', &mut line).await;
        match read {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                error!("reading from the hub failed: {e}");
                break;
            }
        }
        if line.len() > frame::MAX_FRAME_BYTES {
            error!("frame from the hub is too large");
            break;
        }
        let call: WorkerCall = match frame::decode(&line) {
            Ok(call) => call,
            Err(e) => {
                warn!("ignoring a frame that didn't parse: {e}");
                continue;
            }
        };
        let state = state.clone();
        let forwarders = forwarders.clone();
        tokio::spawn(async move {
            let outcome = match handle(&state, &forwarders, call.request).await {
                Ok(response) => WorkerOutcome::Response(response),
                Err(e) => WorkerOutcome::Error(e),
            };
            let _ = state.frames.send(WorkerFrame::Reply { id: call.id, outcome }).await;
        });
    }

    info!("hub stream closed, stopping agents");
    {
        // Waits out a start in flight, so its agent is on the list below.
        let _no_more_starts = state.start_lock.lock().await;
        state.shutting_down.store(true, Ordering::SeqCst);
    }
    stop_setup_script(&state);
    let agents: Vec<_> = state.agents.lock().values().map(|agent| agent.process.clone()).collect();
    let mut stops = JoinSet::new();
    for agent in agents {
        stops.spawn(async move { agent.stop().await });
    }
    stops.join_all().await;
    let mut forwarders = std::mem::take(&mut *forwarders.lock().await);
    while forwarders.join_next().await.is_some() {}
    drop(state);
    drop(frames);
    let _ = writer_task.await;
    Ok(())
}

async fn write_frames<W: AsyncWrite + Unpin>(mut writer: W, mut frames: mpsc::Receiver<WorkerFrame>) {
    while let Some(item) = frames.recv().await {
        let line = match frame::encode(&item) {
            Ok(line) => line,
            Err(e) => {
                error!("could not encode a frame: {e}");
                continue;
            }
        };
        if let Err(e) = writer.write_all(&line).await.and(writer.flush().await) {
            // The hub is gone. Keep draining, so agents stop cleanly rather than blocking on us.
            warn!("writing to the hub failed: {e}");
        }
    }
}

async fn handle(
    state: &Arc<State>,
    forwarders: &Arc<tokio::sync::Mutex<JoinSet<()>>>,
    request: WorkerRequest,
) -> Result<WorkerResponse, ApiError> {
    match request {
        WorkerRequest::Hello { protocol_version } => {
            if protocol_version != PROTOCOL_VERSION {
                return Err(ApiError::new(
                    ErrorCode::VersionMismatch,
                    format!("hub speaks protocol {protocol_version}, this worker speaks {PROTOCOL_VERSION}"),
                ));
            }
            Ok(WorkerResponse::Hello { protocol_version: PROTOCOL_VERSION })
        }
        WorkerRequest::PrepareWorkspace { repo_name, repo_path, workspace_name, branch, base_ref } => {
            let prepared = git::prepare_workspace(
                &state.config.workspaces_root,
                &repo_name,
                &repo_path,
                &workspace_name,
                &branch,
                base_ref.as_deref(),
            )
            .await?;
            Ok(WorkerResponse::WorkspaceReady {
                path: prepared.path,
                branch: prepared.branch,
                base_ref: prepared.base_ref,
                warnings: prepared.warnings,
            })
        }
        WorkerRequest::RestoreWorkspace { repo_path, path, branch } => {
            git::restore_workspace(&repo_path, &path, &branch).await?;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::RemoveWorkspace { repo_path, path, force } => {
            git::remove_workspace(&repo_path, &path, force).await?;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::GitStatus { path } => Ok(WorkerResponse::GitStatus(git::status(&path).await?)),
        WorkerRequest::GitDiff { path, base_ref } => {
            Ok(WorkerResponse::GitDiff(git::diff(&path, base_ref.as_deref()).await?))
        }
        WorkerRequest::PullRequest { path } => {
            Ok(WorkerResponse::PullRequest { pull_request: git::pull_request(&path).await })
        }
        WorkerRequest::SetupWorkspace { repo_path, path, env } => run_setup(state, &repo_path, &path, &env).await,
        WorkerRequest::CancelSetup => {
            stop_setup_script(state);
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::SetupLog { path } => {
            let log = setup::setup_log_path(state.config.state_dir.as_deref(), Path::new(&path));
            Ok(WorkerResponse::SetupLog { text: setup::read_log_tail(&log, SETUP_LOG_TAIL_BYTES) })
        }
        WorkerRequest::ArchiveScript { repo_path, path, env } => {
            run_archive_script(state, &repo_path, &path, &env).await
        }
        WorkerRequest::AgentStart(spec) => start_agent(state, forwarders, spec).await,
        WorkerRequest::AgentPrompt { session_id, text } => {
            let delivered = match state.live_agent(&session_id) {
                Some(agent) => agent.send_prompt(&text).await.is_ok(),
                None => false,
            };
            Ok(if delivered { WorkerResponse::Ok } else { WorkerResponse::AgentNotRunning })
        }
        WorkerRequest::AgentApproval { session_id, approval_id, decision } => {
            let delivered = match state.live_agent(&session_id) {
                Some(agent) => agent.resolve_approval(&approval_id, &decision).await.is_ok(),
                None => false,
            };
            Ok(if delivered { WorkerResponse::Ok } else { WorkerResponse::AgentNotRunning })
        }
        WorkerRequest::AgentCancel { session_id } => {
            let delivered = match state.live_agent(&session_id) {
                Some(agent) => agent.interrupt().await.is_ok(),
                None => false,
            };
            Ok(if delivered { WorkerResponse::Ok } else { WorkerResponse::AgentNotRunning })
        }
        WorkerRequest::AgentConfigure { session_id, model, permission_mode } => {
            let Some(agent) = state.live_agent(&session_id) else {
                return Ok(WorkerResponse::AgentNotRunning);
            };
            match agent.configure(model.as_deref(), permission_mode.as_deref()).await {
                Ok(()) => Ok(WorkerResponse::Ok),
                Err(_) if agent.is_gone() => Ok(WorkerResponse::AgentNotRunning),
                Err(e) => Err(agent_error(&e)),
            }
        }
        WorkerRequest::AgentStop { session_id } => {
            // Includes one that's on its way out: this resolves once it has finished.
            let agent = state.agents.lock().get(&session_id).map(|agent| agent.process.clone());
            match agent {
                Some(agent) => {
                    agent.stop().await;
                    Ok(WorkerResponse::Ok)
                }
                None => Ok(WorkerResponse::AgentNotRunning),
            }
        }
    }
}

/// Signals the script's whole group, because stopping only the shell would leave what it started.
fn stop_setup_script(state: &State) {
    if let Some(pid) = *state.setup_pid.lock() {
        // Its entry is cleared as soon as it has been waited on, so this can't reach a reused PID.
        process::signal_group(pid, process::Signal::Terminate);
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

async fn run_setup(
    state: &Arc<State>,
    repo: &str,
    path: &str,
    env: &[(String, String)],
) -> Result<WorkerResponse, ApiError> {
    let (repo, worktree) = (Path::new(repo), Path::new(path));
    let log = setup::setup_log_path(state.config.state_dir.as_deref(), worktree);
    let _ = std::fs::remove_file(&log);
    let mut warnings = Vec::new();
    let settings = setup::load_settings(repo, worktree, home_dir().as_deref(), &mut warnings);
    let copied = setup::copy_local_files(repo, worktree, &settings, &mut warnings).await;
    for file in &copied {
        setup::append_log(&log, &format!("Copied {file}"));
    }
    for warning in &warnings {
        setup::append_log(&log, &format!("Warning: {warning}"));
    }
    let Some(script) = settings.setup else {
        return Ok(WorkerResponse::SetupDone { code: Some(0) });
    };
    setup::append_log(&log, "Running the setup script");
    // Recorded like an agent, so the provider can stop it if this worker is killed.
    let record = state.config.state_dir.as_ref().map(|dir| dir.join("agents/setup.pid"));
    let (setup_pid, for_start) = (state.setup_pid.clone(), record.clone());
    let status = setup::run_script(&script, worktree, env, &log, move |pid| {
        *setup_pid.lock() = Some(pid);
        if let Some(path) = &for_start {
            let _ = process::ProcessRecord::write(path, pid);
        }
    })
    .await;
    *state.setup_pid.lock() = None;
    if let Some(path) = &record {
        let _ = std::fs::remove_file(path);
    }
    let code = status?.code();
    match code {
        Some(code) => setup::append_log(&log, &format!("The setup script exited with {code}")),
        None => setup::append_log(&log, "The setup script was stopped"),
    }
    Ok(WorkerResponse::SetupDone { code })
}

async fn run_archive_script(
    state: &Arc<State>,
    repo: &str,
    path: &str,
    env: &[(String, String)],
) -> Result<WorkerResponse, ApiError> {
    let (repo, worktree) = (Path::new(repo), Path::new(path));
    if !worktree.exists() {
        return Ok(WorkerResponse::Ok);
    }
    let settings = setup::load_settings(repo, worktree, home_dir().as_deref(), &mut Vec::new());
    let Some(script) = settings.archive else { return Ok(WorkerResponse::Ok) };
    let log = setup::setup_log_path(state.config.state_dir.as_deref(), worktree);
    setup::append_log(&log, "Running the archive script");
    let status = setup::run_script(&script, worktree, env, &log, |_| {}).await?;
    if status.success() {
        return Ok(WorkerResponse::Ok);
    }
    Err(ApiError::new(
        ErrorCode::Conflict,
        format!("the archive script failed with {:?}. Fix it, or archive with force", status.code()),
    ))
}

fn agent_error(e: &anyhow::Error) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("{e:#}"))
}

async fn start_agent(
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
        // Named by run, because a replaced agent removes its own file as it finishes, which can be
        // after its replacement has started.
        pid_file: state.config.state_dir.as_ref().map(|dir| dir.join("agents").join(format!("{session_id}-{run}.pid"))),
    };
    let process = agents::spawn(spec, tx, options).map_err(|e| agent_error(&e))?;
    let pid = process.pid();
    info!(pid, session_id, run, "started agent");
    // Replaces an entry whose process has gone. Its forwarder finishes delivering its last output.
    state.agents.lock().insert(session_id.clone(), RunningAgent { process, run });
    forwarders.lock().await.spawn(forward_output(state.clone(), session_id, run, rx));
    Ok(WorkerResponse::AgentStarted { pid, run, already_running: false })
}

async fn forward_output(state: Arc<State>, session_id: String, run: u64, mut output: mpsc::Receiver<AgentOutput>) {
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
