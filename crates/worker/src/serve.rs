//! The worker's main loop: one stream in from the hub, one stream out.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use agents::AgentProcess;
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::frame::{self, Call};
use protocol::model::{RunId, SessionId};
use protocol::versions::Versions;
use protocol::worker::{ToWorker, WORKER_PROTOCOL_VERSION, WorkerFrame, WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{AbortHandle, JoinSet};
use tracing::{error, info, warn};

use crate::asks::Asks;
use crate::deleting::Deleter;
use crate::tools::Tools;
use crate::{branch_watch, git, setup};

mod agent;
mod scripts;

use agent::{account_for, configure_agent, start_agent, stop_agent, title_for};
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
    /// Deletes the worktrees archiving moves aside.
    pub deleter: Deleter,
    /// Why this worker can't serve its instance, which is another worker serving it already. It answers every request
    /// by saying so, rather than exiting, so the hub can tell it from a worker that failed to start. The hub cleans up
    /// after one of those by ending the instance, which would end the worker serving it.
    pub refused: Option<String>,
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
    /// What's watching the worktree's branches, so a later watch can replace it.
    watching: Mutex<Option<AbortHandle>>,
    frames: mpsc::Sender<WorkerFrame>,
    /// What the worker has asked its hub and waits to hear.
    asks: Arc<Asks>,
    tools: Arc<Tools>,
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
    let asks = Arc::new(Asks::new(frames.clone()));

    let state = Arc::new(State {
        config,
        agents: Mutex::new(HashMap::new()),
        start_lock: tokio::sync::Mutex::new(()),
        shutting_down: AtomicBool::new(false),
        setup: Arc::new(Mutex::new(Setup::Idle)),
        stopping: watch::Sender::new(false),
        calls: AtomicU64::new(0),
        watching: Mutex::new(None),
        frames: frames.clone(),
        tools: Arc::new(Tools::new(asks.clone())),
        asks,
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
        take(&state, &forwarders, &line);
    }

    info!("hub stream closed, stopping agents");
    stop_everything(&state, &forwarders).await;
    drop(state);
    drop(frames);
    let _ = writer_task.await;
}

/// Takes one line from the hub: an answer to something the worker asked, or a call, which is answered on a task of its
/// own.
fn take(state: &Arc<State>, forwarders: &Arc<tokio::sync::Mutex<JoinSet<()>>>, line: &[u8]) {
    let Call { id, request } = match ToWorker::decode(line) {
        Ok(ToWorker::Call(call)) => call,
        Ok(ToWorker::Answer(answer)) => {
            state.asks.answer(answer);
            return;
        }
        Err(e) => {
            warn!("ignoring a frame that didn't parse: {e}");
            return;
        }
    };
    // Each request runs on a task of its own, so a stop read after this could otherwise be handled
    // before the setup it's for has begun.
    if matches!(request, Ok(WorkerRequest::SetupWorkspace { .. })) {
        setup_requested(state);
    }
    let (state, forwarders) = (state.clone(), forwarders.clone());
    tokio::spawn(async move {
        let outcome = handle(&state, &forwarders, request).await.into();
        let _ = state.frames.send(WorkerFrame::Reply { id, outcome }).await;
    });
}

async fn stop_everything(state: &State, forwarders: &tokio::sync::Mutex<JoinSet<()>>) {
    {
        // Waits out a start in flight, so its agent is on the list below.
        let _no_more_starts = state.start_lock.lock().await;
        state.shutting_down.store(true, Ordering::SeqCst);
    }
    state.stopping.send_replace(true);
    state.asks.close();
    // So it lets go of its frames now, rather than once whatever it's reading is done.
    if let Some(watching) = state.watching.lock().take() {
        watching.abort();
    }
    stop_setup_script(state);
    let agents: Vec<_> = state.agents.lock().values().map(|agent| agent.process.clone()).collect();
    let mut stops = JoinSet::new();
    for agent in agents {
        stops.spawn(async move { agent.stop().await });
    }
    stops.join_all().await;
    // After the agents, so their connections close first, and the port isn't left waiting out the server's closes for
    // the next worker to listen on.
    state.tools.stop().await;
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
    request: Result<WorkerRequest, serde_json::Error>,
) -> Result<WorkerResponse, ApiError> {
    if let Some(reason) = &state.config.refused {
        return Err(ApiError::new(ErrorCode::Conflict, reason.clone()));
    }
    // One this build can't read is a newer hub's, which does without whatever this worker refuses, where leaving it
    // unanswered would hold the hub's call until the connection closes.
    match request.map_err(|e| ApiError::unsupported_request("this worker", Some(&e)))? {
        WorkerRequest::Hello { protocol_version, oldest_protocol_version } => {
            hello(protocol_version, oldest_protocol_version)
        }
        WorkerRequest::Unknown => Err(ApiError::unsupported_request("this worker", None)),
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
                branches: prepared.branches,
            })
        }
        WorkerRequest::RestoreWorkspace { repo_path, path, branch } => {
            git::restore_workspace(&repo_path, &path, &branch).await?;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::RemoveWorkspace { repo_path, path, force } => {
            git::remove_workspace(&repo_path, &path, force, &state.config.deleter).await?;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::GitStatus { path } => Ok(WorkerResponse::GitStatus(git::status(&path).await?)),
        WorkerRequest::GitDiff { path, base_ref } => {
            Ok(WorkerResponse::GitDiff(git::diff(&path, base_ref.as_deref()).await?))
        }
        WorkerRequest::PullRequest { path } => {
            // `gh` finds the pull request by the branch checked out.
            let branch = git::branches::checked_out(Path::new(&path)).await;
            Ok(WorkerResponse::PullRequest { pull_request: git::pull_request(&path).await?, branch })
        }
        WorkerRequest::WatchBranches { path } => {
            let (first, sent) = oneshot::channel();
            let watching = branch_watch::watch(path.into(), state.frames.clone(), state.stopping.subscribe(), first);
            if let Some(earlier) = state.watching.lock().replace(tokio::spawn(watching).abort_handle()) {
                earlier.abort();
            }
            // The first frame is queued by then, ahead of this reply, on the same stream.
            let _ = sent.await;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::GithubRateLimit { path } => Ok(WorkerResponse::GithubRateLimit(git::rate_limit(&path).await?)),
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
        WorkerRequest::ListFiles { path } => Ok(WorkerResponse::Files(crate::files::list(Path::new(&path)).await?)),
        WorkerRequest::FolderRead { path, folder } => {
            let files = tokio::task::spawn_blocking(move || crate::files::read_folder(Path::new(&path), &folder))
                .await
                .map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading a folder stopped: {e}")))??;
            Ok(WorkerResponse::Files(files))
        }
        WorkerRequest::FileRead { path, file } => {
            let contents = tokio::task::spawn_blocking(move || crate::files::read(Path::new(&path), &file))
                .await
                .map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading a file stopped: {e}")))??;
            Ok(WorkerResponse::FileContents(contents))
        }
        WorkerRequest::FileReadAt { path, commit, file } => {
            Ok(WorkerResponse::FileContents(crate::files::read_at(Path::new(&path), &commit, &file).await?))
        }
        WorkerRequest::FileOpenAt { path, commit, file } => {
            Ok(WorkerResponse::FileOpened(crate::files::open_at(Path::new(&path), &commit, &file).await?))
        }
        WorkerRequest::FilePages { path, commit, file, page_lines, pages } => {
            let pages = crate::files::pages(Path::new(&path), &commit, &file, page_lines, &pages).await?;
            Ok(WorkerResponse::FilePages(pages))
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
        WorkerRequest::AgentCompact { session_id, asked, keeping } => {
            let agent = state.live_agent(&session_id);
            Ok(delivery(match agent {
                Some(agent) => agent.compact(asked, keeping.as_deref()).await.is_ok(),
                None => false,
            }))
        }
        WorkerRequest::AgentConfigure { session_id, options } => configure_agent(state, &session_id, &options).await,
        WorkerRequest::AgentStop { session_id } => stop_agent(state, &session_id).await,
        WorkerRequest::TitleFor { agent, prompt, images } => title_for(state, agent, &prompt, &images).await,
        WorkerRequest::AccountFor { agent } => account_for(state, agent).await,
        WorkerRequest::ServeTools { port } => Ok(WorkerResponse::ToolsServing { port: state.tools.serve(port).await? }),
    }
}

/// A worker speaks only its own version, and takes a hub whose hello names it, since a hub speaks the one before its
/// own too once a protocol breaks, as "Changing the protocol" in `docs/architecture.md` says.
fn hello(newest: u32, oldest: Option<u32>) -> Result<WorkerResponse, ApiError> {
    let (ours, hub) = (Versions::only(WORKER_PROTOCOL_VERSION), Versions::from_hello(newest, oldest));
    let Some(chosen) = ours.newest_shared(hub) else {
        let message = format!("the hub speaks {hub}, and this worker speaks {ours}");
        return Err(ApiError::new(ErrorCode::VersionMismatch, message));
    };
    Ok(WorkerResponse::Hello { protocol_version: chosen })
}

fn delivery(delivered: bool) -> WorkerResponse {
    if delivered { WorkerResponse::Ok } else { WorkerResponse::AgentNotRunning }
}
