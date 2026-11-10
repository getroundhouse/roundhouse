//! The worker's main loop: one stream in from the hub, one stream out.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use agents::AgentProcess;
use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::attachment::AttachmentBytes;
use protocol::client::{ApiError, ErrorCode};
use protocol::frame;
use protocol::model::{RunId, SessionId};
use protocol::worker::{WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::{git, setup};

mod agent;
mod scripts;

use agent::{configure_agent, start_agent, stop_agent, title_for};
use scripts::{Setup, run_archive_script, run_setup, setup_requested, stop_setup_script};

/// Enough to see what went wrong, without sending a whole install log across.
const SETUP_LOG_TAIL_BYTES: u64 = 256 * 1024;

pub struct Config {
    pub workspaces_root: PathBuf,
    /// Where to record running agents, so they can be found if this process is killed.
    pub state_dir: Option<PathBuf>,
    /// Holds the user's own settings, and the places agent CLIs install to.
    pub home: Option<PathBuf>,
    /// Where to search for agent CLIs.
    pub path: Option<OsString>,
    /// An agent CLI to use without searching.
    pub claude_bin: Option<PathBuf>,
    /// Where a one-off call to an agent runs, each in an empty directory of its own. The system's temporary
    /// directory suits, since no project's instructions are above it.
    pub scratch_dir: PathBuf,
}

struct RunningAgent {
    process: Arc<dyn AgentProcess>,
    run: RunId,
}

struct State {
    config: Config,
    agents: Mutex<HashMap<SessionId, RunningAgent>>,
    /// Serializes starts, so two requests can't launch two processes for one conversation.
    start_lock: tokio::sync::Mutex<()>,
    /// Set under `start_lock` once the hub has gone, so a start still in flight can't add an agent
    /// after the worker has decided what to stop.
    shutting_down: AtomicBool,
    /// The setup script, so it can be stopped.
    setup: Arc<Mutex<Setup>>,
    /// Set once the worker is stopping, so a one-off call under way gives up rather than keep it running.
    stopping: watch::Sender<bool>,
    /// How many one-off calls have started, which numbers their records.
    calls: AtomicU64,
    frames: mpsc::Sender<WorkerFrame>,
}

impl State {
    /// The session's agent, unless its process has gone. Its entry can outlast the process by a
    /// while, because its last output is still on the way to the hub.
    fn live_agent(&self, session_id: &SessionId) -> Option<Arc<dyn AgentProcess>> {
        let agents = self.agents.lock();
        agents.get(session_id).filter(|agent| !agent.process.is_gone()).map(|agent| agent.process.clone())
    }
}

/// Runs until the hub closes the stream, then stops every agent and flushes what they said. Losing
/// the hub ends the worker on purpose, so agents never run with nobody recording them. Milestone 2
/// changes this, with an outbox that lets remote workers keep going.
pub async fn serve<R, W>(reader: R, writer: W, config: Config)
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
        setup: Arc::new(Mutex::new(Setup::Idle)),
        stopping: watch::Sender::new(false),
        calls: AtomicU64::new(0),
        frames: frames.clone(),
    });
    let forwarders = Arc::new(tokio::sync::Mutex::new(JoinSet::new()));

    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        frame::clear_buffer(&mut line);
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
        // Each request runs on a task of its own, so a stop read after this could otherwise be handled
        // before the setup it's for has begun.
        if matches!(call.request, WorkerRequest::SetupWorkspace { .. }) {
            setup_requested(&state);
        }
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
    stop_everything(&state, &forwarders).await;
    drop(state);
    drop(frames);
    let _ = writer_task.await;
}

async fn stop_everything(state: &State, forwarders: &tokio::sync::Mutex<JoinSet<()>>) {
    {
        // Waits out a start in flight, so its agent is on the list below.
        let _no_more_starts = state.start_lock.lock().await;
        state.shutting_down.store(true, Ordering::SeqCst);
    }
    state.stopping.send_replace(true);
    stop_setup_script(state);
    let agents: Vec<_> = state.agents.lock().values().map(|agent| agent.process.clone()).collect();
    let mut stops = JoinSet::new();
    for agent in agents {
        stops.spawn(async move { agent.stop().await });
    }
    stops.join_all().await;
    let mut forwarders = std::mem::take(&mut *forwarders.lock().await);
    while forwarders.join_next().await.is_some() {}
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

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
async fn handle(
    state: &Arc<State>,
    forwarders: &Arc<tokio::sync::Mutex<JoinSet<()>>>,
    request: WorkerRequest,
) -> Result<WorkerResponse, ApiError> {
    match request {
        WorkerRequest::Hello { protocol_version } => hello(protocol_version),
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
        WorkerRequest::AttachmentWrite { path, name, data } => {
            let written = tokio::task::spawn_blocking(move || {
                crate::attachments::write(Path::new(&path), &name, data.as_slice())
            })
            .await
            .map_err(|e| ApiError::new(ErrorCode::Internal, format!("writing an attachment stopped: {e}")))??;
            Ok(WorkerResponse::AttachmentWritten { path: written.to_string_lossy().into_owned() })
        }
        WorkerRequest::ImageRead { path, image } => {
            let (media_type, data) = tokio::task::spawn_blocking(move || crate::images::read(Path::new(&path), &image))
                .await
                .map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading an image stopped: {e}")))??;
            Ok(WorkerResponse::Image { media_type, data: AttachmentBytes::new(data) })
        }
        WorkerRequest::AgentStart(spec) => start_agent(state, forwarders, spec).await,
        WorkerRequest::AgentPrompt { session_id, prompt, text, attachments } => {
            let agent = state.live_agent(&session_id);
            Ok(delivery(match agent {
                Some(agent) => agent.send_prompt(prompt, &text, &attachments).await.is_ok(),
                None => false,
            }))
        }
        WorkerRequest::AgentApproval { session_id, approval_id, decision } => {
            let agent = state.live_agent(&session_id);
            Ok(delivery(match agent {
                Some(agent) => agent.resolve_approval(&approval_id, &decision).await.is_ok(),
                None => false,
            }))
        }
        WorkerRequest::AgentCancel { session_id } => {
            let agent = state.live_agent(&session_id);
            Ok(delivery(match agent {
                Some(agent) => agent.interrupt().await.is_ok(),
                None => false,
            }))
        }
        WorkerRequest::AgentConfigure { session_id, options } => configure_agent(state, &session_id, &options).await,
        WorkerRequest::AgentStop { session_id } => stop_agent(state, &session_id).await,
        WorkerRequest::TitleFor { agent, prompt } => title_for(state, agent, &prompt).await,
    }
}

fn hello(protocol_version: u32) -> Result<WorkerResponse, ApiError> {
    if protocol_version != PROTOCOL_VERSION {
        return Err(ApiError::new(
            ErrorCode::VersionMismatch,
            format!("hub speaks protocol {protocol_version}, this worker speaks {PROTOCOL_VERSION}"),
        ));
    }
    Ok(WorkerResponse::Hello { protocol_version: PROTOCOL_VERSION })
}

fn delivery(delivered: bool) -> WorkerResponse {
    if delivered { WorkerResponse::Ok } else { WorkerResponse::AgentNotRunning }
}
