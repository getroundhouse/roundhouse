//! Starting and stopping agents, forwarding what they say to the hub, and the one-off calls that title a prompt and
//! read the agent's account.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use agents::{AgentOptions, AgentOutput, BinaryLookup, ToolServer};
use protocol::attachment::NewAttachment;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{AgentKind, AgentLogin, RunId, SessionId, SessionSettingsChange};
use protocol::paths::AGENTS_DIR;
use protocol::prompt::Prompt;
use protocol::worker::{AgentSpec, AgentStarted, TitleReply, WorkerFrame, WorkerResponse};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{info, warn};

use super::{RunningAgent, State};
use crate::context_dir::Folder;
use crate::instructions::INSTRUCTIONS;

/// Gives up once the worker is stopping, which ends the call, so a title nobody will see can't keep it running.
pub(crate) async fn title_for(
    state: &State,
    agent: AgentKind,
    prompt: &Prompt<NewAttachment>,
    login: Option<AgentLogin>,
) -> Result<WorkerResponse, ApiError> {
    let options = one_off(state, "title", login);
    let title = agents::title_for(agent, prompt, &options, &state.config.scratch_dir);
    let title = until_stopping(state, title).await?;
    Ok(WorkerResponse::Title(TitleReply { title: title?.to_string() }))
}

/// What a one-off call to an agent's CLI runs with: a record of its own, numbered, so the provider can stop the call
/// if this worker is killed, and the login its hub sent with it, which goes with the call.
fn one_off(state: &State, what: &str, login: Option<AgentLogin>) -> AgentOptions {
    let call = state.calls.fetch_add(1, Ordering::Relaxed);
    let record = |dir: &std::path::PathBuf| dir.join(AGENTS_DIR).join(format!("{what}-{call}.pid"));
    AgentOptions {
        binary: binary(state),
        pid_file: state.config.state_dir.as_ref().map(record),
        tools: None,
        instructions: None,
        login,
    }
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

/// The account the agent's CLI runs under here, with `login` or with this machine's own.
pub(crate) async fn account_for(
    state: &State,
    agent: AgentKind,
    login: Option<AgentLogin>,
) -> Result<WorkerResponse, ApiError> {
    let options = one_off(state, "account", login);
    let account = agents::account(agent, &options, &state.config.scratch_dir);
    Ok(WorkerResponse::Account(until_stopping(state, account).await??))
}

pub(crate) async fn start_agent(
    state: &Arc<State>,
    forwarders: &Arc<tokio::sync::Mutex<JoinSet<()>>>,
    mut spec: AgentSpec,
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
        return Ok(WorkerResponse::AgentStarted(AgentStarted {
            pid: agent.process.pid(),
            run: agent.run,
            already_running: true,
        }));
    }
    let (tx, rx) = mpsc::channel(1024);
    // Named by run, because a replaced agent removes its own files as it finishes, which can be after its replacement
    // has started, and a CLI whose `--mcp-config` file has gone doesn't start at all.
    let agent_file = |extension: &str| {
        state.config.state_dir.as_ref().map(|dir| dir.join(AGENTS_DIR).join(format!("{session_id}-{run}.{extension}")))
    };
    make_tmp_folder(&session_id, &worktree).await;
    let options = AgentOptions {
        binary: binary(state),
        pid_file: agent_file("pid"),
        tools: tools_for(state, &mut spec, agent_file("mcp.json")).await,
        instructions: Some(INSTRUCTIONS.to_string()),
        // Taken out of the launch, as the tools' token is, so it's in one place: what the agent is started with, and
        // started again with, which goes when the agent does.
        login: spec.login.take(),
    };
    let process = agents::spawn(spec, tx, options).await?;
    let pid = process.pid();
    info!(pid, %session_id, %run, "started agent");
    // Replaces an entry whose process has gone. Its forwarder finishes delivering its last output.
    state.agents.lock().insert(session_id.clone(), RunningAgent { process, run });
    forwarders.lock().await.spawn(forward_output(state.clone(), session_id, run, worktree, rx));
    Ok(WorkerResponse::AgentStarted(AgentStarted { pid, run, already_running: false }))
}

/// Makes the folder the agent's instructions send it to for what it makes for its own use. An agent can work without
/// it, though what it keeps there may then show as changes, or go elsewhere, so failing is logged rather than stopping
/// the start.
async fn make_tmp_folder(session_id: &SessionId, worktree: &Path) {
    let at = worktree.to_path_buf();
    let made = tokio::task::spawn_blocking(move || Folder::Tmp.make(&at).map(drop))
        .await
        .unwrap_or_else(|e| Err(std::io::Error::other(e)));
    if let Err(e) = made {
        warn!(%session_id, worktree = %worktree.display(), "couldn't make the agent's tmp folder: {e}");
    }
}

/// Where the agent reaches the worker's tools: at the address they're served on, with the token its hub gave it, which
/// lets its calls through from then on. `None` when the hub gave it none, when they aren't served, or when there's no
/// state directory to tell its CLI in, at `config`, and it goes without them.
async fn tools_for(state: &State, spec: &mut AgentSpec, config: Option<PathBuf>) -> Option<ToolServer> {
    let token = spec.tools_token.take()?;
    let session_id = &spec.session_id;
    let Some(url) = state.tools.url().await else {
        warn!(%session_id, "the agent goes without the worker's tools, which aren't being served");
        return None;
    };
    let Some(config) = config else {
        warn!(%session_id, "the agent goes without the worker's tools, with no state directory to tell its CLI in");
        return None;
    };
    state.tools.admit(token.clone(), session_id.clone());
    Some(ToolServer { url, token, config })
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
                WorkerFrame::AgentEvent { session_id: session_id.clone(), run, body, raw, attachments, seq: None }
            }
            AgentOutput::Delta(delta) => WorkerFrame::AgentDelta { session_id: session_id.clone(), run, delta },
            AgentOutput::Context(context) => {
                WorkerFrame::AgentContext { session_id: session_id.clone(), run, context, seq: None }
            }
            AgentOutput::Cache(request) => {
                WorkerFrame::AgentCache { session_id: session_id.clone(), run, request, seq: None }
            }
            AgentOutput::Limits(limits) => WorkerFrame::AgentLimits {
                session_id: session_id.clone(),
                run,
                limits,
                every_models_week: false,
                seq: None,
            },
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
                    seq: None,
                }
            }
        };
        state.outbox.send(frame).await;
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
