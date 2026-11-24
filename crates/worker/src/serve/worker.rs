use super::{agent, connection, moves, outbox, scripts, worktrees};
use crate::asks::Asks;
use crate::branch_watch::Whose;
use crate::deleting::Deleter;
use crate::git::{ChangeSummaries, Clones};
use crate::git_credential::{CredentialHelper, GitCredentials};
use crate::terminals::Terminals;
use crate::tools::Tools;
use crate::{git, setup};
use agent::account_for;
use agent::{start_agent, stop_agent, title_for};
use agents::AgentProcess;
use connection::Closing;
use moves::Moves;
use outbox::Outbox;
use parking_lot::Mutex;
use protocol::Build;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ByteRange, RunId, SessionId};
use protocol::paths::AGENTS_DIR;
use protocol::prompt::Prompt;
use protocol::worker::{
    AccountFor, AgentStop, AttachmentWrite, AttachmentWritten, FileOpenAt, FilePages, FileRead, FileReadAt,
    FileReadRange, FolderRead, GitDiff, GitStatus, ListChangedFiles, ReadRepoUrl, Received, RunningAgentsReply,
    ServeTools, SetupLog, SetupLogReply, SummarizeChanges, TerminalStop, TitleFor, ToolsServing, WorkerFrame,
    WorkerRequest, WorkerResponse,
};
use scripts::{Setup, run_archive_script, run_setup, stop_setup_script};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::info;
use worktrees::Watches;

/// Enough to see what went wrong, without sending a whole install log across.
const SETUP_LOG_TAIL_BYTES: u64 = 256 * 1024;
/// Enough for every agent to be waiting on an answer at once.
const QUEUED_ASKS: usize = 64;

pub struct Config {
    /// Which build this worker is, which its hello tells the hub.
    pub build: Build,
    pub workspaces_root: PathBuf,
    /// The worker's state directory, where it records what it runs, so it can be stopped if this process is killed, and
    /// keeps the setup log and each run's files. It's there before the worker serves, as claiming the instance makes
    /// it.
    pub state_dir: PathBuf,
    /// Where the worker keeps its clones of the repositories it's told of by their address, which every worker on the
    /// machine that its provider runs shares, and which outlives any instance. Its provider names it, as it does the
    /// state directory.
    pub clones_dir: PathBuf,
    /// Holds the user's own settings, and the places agent CLIs install to.
    pub home: Option<PathBuf>,
    /// Where to search for agent CLIs.
    pub path: Option<OsString>,
    /// An agent CLI to use without searching.
    pub claude_bin: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
    pub antigravity_bin: Option<PathBuf>,
    /// Where a one-off call to an agent runs, each in an empty directory of its own. The system's temporary
    /// directory suits, since no project's instructions are above it.
    pub scratch_dir: PathBuf,
    /// Deletes the worktrees archiving moves aside.
    pub deleter: Deleter,
    /// What git runs for a credential for github.com, in every process the worker gives the git credential its hub
    /// set.
    pub credential_helper: CredentialHelper,
    /// What every process the worker starts is given beyond what it inherits from the worker, as the machine's own,
    /// before the git credential its hub sets: `Environment::NONE` in the product, which leaves it the worker's own
    /// environment, and a test's own whole environment in a test. Whatever it is, the worker takes git's variables of
    /// a repository out of it, as `git::in_no_repository` says, since each workspace is a repository of its own.
    pub machine: process::Environment,
    /// Why this worker can't serve its instance, which is another worker serving it already. It answers every request
    /// by saying so, rather than exiting, so the hub can tell it from a worker that failed to start. The hub cleans up
    /// after one of those by ending the instance, which would end the worker serving it.
    pub refused: Option<String>,
    pub timers: crate::Timers,
}

pub(super) struct RunningAgent {
    pub(super) process: Arc<dyn AgentProcess>,
    pub(super) run: RunId,
}

pub(super) struct State {
    pub(super) config: Config,
    pub(super) agents: Mutex<HashMap<SessionId, RunningAgent>>,
    /// Serializes starts, so two requests can't launch two processes for one conversation.
    pub(super) start_lock: tokio::sync::Mutex<()>,
    /// Set under `start_lock` once the worker is stopping, so a start still in flight can't add an agent after the
    /// worker has decided what to stop.
    pub(super) shutting_down: AtomicBool,
    /// The setup script, so it can be stopped.
    pub(super) setup: Arc<Mutex<Setup>>,
    /// Set once the worker is stopping, so a one-off call under way gives up rather than keep it running.
    pub(super) stopping: watch::Sender<bool>,
    /// How many one-off calls have started, which numbers their records.
    pub(super) calls: AtomicU64,
    /// One at a time for each worktree: making another of the workspace's, removing any, or putting the workspace's
    /// own back, each of which goes on when the hub that asked has gone, and the next hub's question of whether one is
    /// there, so the answer is how that ended.
    pub(super) moves: Moves,
    pub(super) watches: Mutex<Watches>,
    /// What agents say, what the worker asks, what happens in its terminals, and the branches it watches, on its way
    /// to the hub.
    pub(super) outbox: Arc<Outbox>,
    /// What the worker has asked its hub and waits to hear.
    pub(super) asks: Arc<Asks>,
    pub(super) tools: Arc<Tools>,
    /// The git credential its hub set, which every process it starts is given.
    pub(super) credentials: GitCredentials,
    pub(super) clones: Clones,
    /// What the worker last read of each worktree's changed files, so a summary reads only those that changed since.
    pub(super) changes: ChangeSummaries,
    pub(super) terminals: Terminals,
}

impl State {
    /// The session's agent, unless its process has gone. Its entry can outlast the process by a
    /// while, because its last output is still on the way to the hub.
    pub(super) fn live_agent(&self, session_id: &SessionId) -> Option<Arc<dyn AgentProcess>> {
        let agents = self.agents.lock();
        agents.get(session_id).filter(|agent| !agent.process.is_gone()).map(|agent| agent.process.clone())
    }
}

/// What a worker keeps for as long as it runs, whichever hub is connected.
#[derive(Clone)]
pub(super) struct Worker {
    pub(super) state: Arc<State>,
    /// Each agent's forwarder, so stopping can wait for what the agents said as they stopped.
    pub(super) forwarders: Arc<tokio::sync::Mutex<JoinSet<()>>>,
}

impl Worker {
    pub(super) fn new(config: Config) -> Self {
        let outbox = Arc::new(Outbox::default());
        // What the worker asks goes the way what agents say does, to whichever hub is connected. It isn't kept, since
        // the asks are asked again of each hub that connects until they're answered.
        let (asking, asked) = mpsc::channel::<WorkerFrame>(QUEUED_ASKS);
        tokio::spawn(send_asks(asked, outbox.clone()));
        let asks = Arc::new(Asks::new(asking));
        let credentials = GitCredentials::new(&config.state_dir, &config.credential_helper, config.machine.clone());
        // What the last worker left, if it was killed. Not a refused worker's, since the state directory is another
        // worker's, which is serving it.
        if config.refused.is_none() {
            credentials.remove();
        }
        let state = Arc::new(State {
            agents: Mutex::new(HashMap::new()),
            start_lock: tokio::sync::Mutex::new(()),
            shutting_down: AtomicBool::new(false),
            setup: Arc::new(Mutex::new(Setup::Idle)),
            stopping: watch::Sender::new(false),
            calls: AtomicU64::new(0),
            moves: Moves::default(),
            watches: Mutex::new(Watches::default()),
            terminals: Terminals::new(outbox.clone(), config.state_dir.join(AGENTS_DIR), config.timers),
            outbox,
            tools: Arc::new(Tools::new(asks.clone(), config.build, config.timers.wait_poll)),
            clones: Clones::new(config.clones_dir.clone()),
            changes: ChangeSummaries::default(),
            config,
            asks,
            credentials,
        });
        Self { state, forwarders: Arc::new(tokio::sync::Mutex::new(JoinSet::new())) }
    }

    /// Stops every agent, terminal and the setup script, and sends what the agents said and the terminals' ends to the
    /// hub on `last`, if there's one.
    pub(super) async fn stop(&self, last: Option<Closing>) {
        stop_everything(&self.state, &self.forwarders).await;
        self.state.outbox.detach().await;
        if let Some(last) = last {
            last.finish().await;
        }
    }
}

/// Serves the hub on `reader` and `writer`, which are the worker's stdin and stdout, until the hub closes them, then
/// stops every agent and terminal and sends the hub what they said as they stopped. They're the hub's pipes, so a
/// worker on them can't outlive its hub, and it stops whatever the hub asked. One that can is served by `listen`.
pub async fn serve<R, W>(reader: R, writer: W, config: Config)
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let refused = config.refused.clone();
    let worker = Worker::new(config);
    let last = connection::serve(&worker, reader, writer, refused).await;
    info!("hub stream closed, stopping agents");
    worker.stop(Some(last)).await;
}

/// Until the worker has gone, and with it every way to ask.
async fn send_asks(mut asked: mpsc::Receiver<WorkerFrame>, outbox: Arc<Outbox>) {
    while let Some(ask) = asked.recv().await {
        outbox.send(ask).await;
    }
}

async fn stop_everything(state: &State, forwarders: &tokio::sync::Mutex<JoinSet<()>>) {
    {
        // Waits out a start in flight, so its agent is on the list below.
        let _no_more_starts = state.start_lock.lock().await;
        state.shutting_down.store(true, Ordering::SeqCst);
    }
    state.stopping.send_replace(true);
    state.asks.close();
    state.watches.lock().stop_all();
    stop_setup_script(state);
    let agents: Vec<_> = state.agents.lock().values().map(|agent| agent.process.clone()).collect();
    let mut stops = JoinSet::new();
    for agent in agents {
        stops.spawn(async move { agent.stop().await });
    }
    tokio::join!(stops.join_all(), state.terminals.stop_all());
    // After the agents, so their connections close first, and the port isn't left waiting out the server's closes for
    // the next worker to listen on.
    state.tools.stop().await;
    let mut forwarders = std::mem::take(&mut *forwarders.lock().await);
    while forwarders.join_next().await.is_some() {}
    // Once nothing it started is left to read them.
    if state.config.refused.is_none() {
        state.credentials.remove();
    }
}

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
pub(super) async fn handle(
    state: &Arc<State>,
    forwarders: &Arc<tokio::sync::Mutex<JoinSet<()>>>,
    request: Result<WorkerRequest, serde_json::Error>,
) -> Result<WorkerResponse, ApiError> {
    // One this build can't read is a newer hub's, which does without whatever this worker refuses, where leaving it
    // unanswered would hold the hub's call until the connection closes.
    let build = state.config.build.version();
    let unsupported =
        |unreadable: Option<&serde_json::Error>| ApiError::unsupported_request("this worker", build, unreadable);
    // What every process this request starts is given, as the hub last set it before sending this.
    let given = state.credentials.given();
    match request.map_err(|e| unsupported(Some(&e)))? {
        // About the connection they came on, or to happen in the order they came, which it answers itself.
        WorkerRequest::Hello(_)
        | WorkerRequest::Stop
        | WorkerRequest::GitCredentialSet(_)
        | WorkerRequest::TerminalTyped(_)
        | WorkerRequest::TerminalFit(_) => {
            Err(ApiError::new(ErrorCode::Internal, "the connection answers this itself"))
        }
        WorkerRequest::Received(Received { through }) => {
            state.outbox.received(through).await;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::RunningAgents => Ok(running_agents(state)),
        WorkerRequest::Unknown => Err(unsupported(None)),
        WorkerRequest::TerminalStart(start) => state.terminals.start(start, given).await.map(|()| WorkerResponse::Ok),
        WorkerRequest::TerminalRows(asked) => Ok(WorkerResponse::TerminalRows(state.terminals.rows(asked).await?)),
        WorkerRequest::TerminalBounds(asked) => {
            Ok(WorkerResponse::TerminalBounds(state.terminals.bounds(asked).await?))
        }
        WorkerRequest::TerminalText(asked) => Ok(WorkerResponse::TerminalText(state.terminals.text(asked).await?)),
        WorkerRequest::TerminalWatch(watch) => {
            state.terminals.watch(watch, state.outbox.connection().await)?;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::TerminalStop(TerminalStop { terminal }) => {
            state.terminals.stop(&terminal).await;
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::RunningTerminals => Ok(WorkerResponse::RunningTerminals(state.terminals.running())),
        WorkerRequest::NamesGitHas(asked) => worktrees::names_git_has(state, asked).await,
        WorkerRequest::PrepareWorkspace(asked) => worktrees::prepare(state, asked).await,
        WorkerRequest::CaptureArchive(asked) => worktrees::capture_archive(state, asked).await,
        WorkerRequest::RestoreWorkspace(asked) => worktrees::restore(state, asked).await,
        WorkerRequest::RemoveWorkspace(asked) => worktrees::remove(state, asked).await,
        WorkerRequest::WorktreeThere(asked) => worktrees::there(state, asked).await,
        WorkerRequest::AddChildWorktree(asked) => worktrees::add_child(state, asked).await,
        WorkerRequest::GitStatus(GitStatus { path }) => {
            Ok(WorkerResponse::GitStatus(git::status(&given, &path).await?))
        }
        WorkerRequest::GitDiff(GitDiff { path, base_ref }) => {
            Ok(WorkerResponse::GitDiff(git::diff(&given, &path, base_ref.as_deref()).await?))
        }
        WorkerRequest::SummarizeChanges(SummarizeChanges { path, base_ref }) => {
            let summary = state.changes.summarize(&given, Path::new(&path), base_ref.as_deref()).await?;
            Ok(WorkerResponse::ChangeSummary(summary))
        }
        WorkerRequest::ListChangedFiles(ListChangedFiles { path, base_ref, after }) => {
            let page = state.changes.page_after(&given, Path::new(&path), base_ref.as_deref(), &after).await?;
            Ok(WorkerResponse::ChangedFilesPage(page))
        }
        WorkerRequest::DiffFile(asked) => Ok(WorkerResponse::FileDiff(git::diff_file(&given, &asked).await?)),
        WorkerRequest::WatchBranches(asked) => worktrees::watch_branches(state, asked.path, Whose::Own).await,
        WorkerRequest::WatchChanges(asked) => worktrees::watch_changes(state, asked).await,
        WorkerRequest::WatchChildBranches(asked) => worktrees::watch_branches(state, asked.path, Whose::Child).await,
        // The machine's own too, since the git credential's rewriting of GitHub's addresses is the worker's, not the
        // checkout's.
        WorkerRequest::ReadRepoUrl(ReadRepoUrl { path }) => {
            Ok(WorkerResponse::RepoUrl(git::repo_url(state.credentials.machine(), Path::new(&path)).await?))
        }
        WorkerRequest::SetupWorkspace(asked) => run_setup(state, &asked.source, &asked.path, &asked.env).await,
        WorkerRequest::CancelSetup => {
            stop_setup_script(state);
            Ok(WorkerResponse::Ok)
        }
        WorkerRequest::SetupLog(SetupLog { path: _ }) => {
            let log = setup::setup_log_path(&state.config.state_dir);
            Ok(WorkerResponse::SetupLog(SetupLogReply { text: setup::read_log_tail(&log, SETUP_LOG_TAIL_BYTES) }))
        }
        WorkerRequest::ArchiveScript(asked) => run_archive_script(state, &asked.source, &asked.path, &asked.env).await,
        WorkerRequest::AttachmentWrite(AttachmentWrite { path, name, data }) => {
            let written = tokio::task::spawn_blocking(move || {
                crate::attachments::write(Path::new(&path), &name, data.as_slice())
            })
            .await
            .map_err(|e| ApiError::new(ErrorCode::Internal, format!("writing an attachment stopped: {e}")))??;
            Ok(WorkerResponse::AttachmentWritten(AttachmentWritten { path: written.to_string_lossy().into_owned() }))
        }
        WorkerRequest::ListFiles(asked) => {
            Ok(WorkerResponse::Files(crate::files::list(&given, Path::new(&asked.path)).await?))
        }
        WorkerRequest::FolderRead(FolderRead { path, folder }) => {
            let files = tokio::task::spawn_blocking(move || crate::files::read_folder(Path::new(&path), &folder))
                .await
                .map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading a folder stopped: {e}")))??;
            Ok(WorkerResponse::Files(files))
        }
        WorkerRequest::FileRead(FileRead { path, file }) => {
            let contents = tokio::task::spawn_blocking(move || crate::files::read(Path::new(&path), &file))
                .await
                .map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading a file stopped: {e}")))??;
            Ok(WorkerResponse::FileContents(contents))
        }
        WorkerRequest::FileReadRange(FileReadRange { path, file, start, length }) => {
            let range = ByteRange::new(start, length)?;
            let part = tokio::task::spawn_blocking(move || crate::files::read_range(Path::new(&path), &file, range))
                .await
                .map_err(|e| ApiError::new(ErrorCode::Internal, format!("reading part of a file stopped: {e}")))??;
            Ok(WorkerResponse::FileRange(part))
        }
        WorkerRequest::FileReadAt(FileReadAt { path, commit, file }) => {
            Ok(WorkerResponse::FileContents(crate::files::read_at(&given, Path::new(&path), &commit, &file).await?))
        }
        WorkerRequest::FileOpenAt(FileOpenAt { path, commit, file }) => {
            Ok(WorkerResponse::FileOpened(crate::files::open_at(&given, Path::new(&path), &commit, &file).await?))
        }
        WorkerRequest::FilePages(FilePages { path, commit, file, page_lines, pages }) => {
            let pages = crate::files::pages(&given, Path::new(&path), &commit, &file, page_lines, &pages).await?;
            Ok(WorkerResponse::FilePages(pages))
        }
        WorkerRequest::AgentControl(request) => {
            let Some(agent) = state.live_agent(&request.session_id) else {
                return Ok(WorkerResponse::AgentNotRunning);
            };
            if !request.control.allowed(&agent.capabilities()) {
                return Err(ApiError::new(ErrorCode::Unsupported, "the run does not offer this control"));
            }
            match agent.control(&request.control).await {
                Ok(()) => Ok(WorkerResponse::Ok),
                Err(_) if agent.is_gone() => Ok(WorkerResponse::AgentNotRunning),
                Err(agents::AgentError::AfterTheTurn) => Ok(WorkerResponse::AfterTheTurn),
                Err(e) => Err(e.into()),
            }
        }
        WorkerRequest::AgentStart(spec) => start_agent(state, forwarders, spec).await,
        WorkerRequest::AgentStop(AgentStop { session_id }) => stop_agent(state, &session_id).await,
        WorkerRequest::TitleFor(TitleFor { agent, prompt, images, login }) => {
            title_for(state, agent, &Prompt::carrying(prompt, images), login).await
        }
        WorkerRequest::AccountFor(AccountFor { agent, login }) => account_for(state, agent, login).await,
        WorkerRequest::ServeTools(ServeTools { port }) => {
            Ok(WorkerResponse::ToolsServing(ToolsServing { port: state.tools.serve(port).await? }))
        }
    }
}

/// Those whose process hasn't gone, which is every one whose session a hub connecting again can carry on with.
fn running_agents(state: &State) -> WorkerResponse {
    let agents = state.agents.lock();
    let running = agents.iter().filter(|(_, agent)| !agent.process.is_gone());
    let listed = |(session_id, agent): (&SessionId, &RunningAgent)| protocol::worker::RunningAgent {
        session_id: session_id.clone(),
        run: agent.run,
        capabilities: Box::new(agent.process.capabilities()),
    };
    let agents = running.map(listed);
    WorkerResponse::RunningAgents(RunningAgentsReply { agents: agents.collect() })
}
