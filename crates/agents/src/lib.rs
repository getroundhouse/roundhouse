//! Agent adapters. Each one runs an agent as a child process and turns its output into normalized
//! events, while keeping every raw line so those events can be rebuilt later.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod claude;
mod error;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use protocol::attachment::{AttachmentFile, NewAttachment};
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{
    AgentAccount, AgentDescription, AgentKind, AgentLogin, ApprovalId, CacheRequest, ContextUsage, Seq,
    SessionSettingsChange, Title, UsageLimit,
};
use protocol::prompt::Prompt;
use protocol::tail::StreamDelta;
use protocol::worker::{AgentSpec, ToolsToken};
use tokio::sync::mpsc;

pub use error::{AgentError, CallFailed, NoAccount, NoTitle};

#[derive(Debug)]
pub enum AgentOutput {
    Event {
        body: EventBody,
        raw: Option<String>,
        /// Images the event names, such as one a tool sent back, for the hub to keep. `raw` is without their bytes.
        attachments: Vec<NewAttachment>,
    },
    Delta(StreamDelta),
    /// How full the agent's context is now. It answers the adapter's own question rather than being something the
    /// agent said, and only the newest matters, so it isn't an event.
    Context(ContextUsage),
    /// One of the agent's own requests to its model, as the model started to answer it. It isn't an event, since a
    /// turn makes many, but each matters, since the hub compares each with the one before.
    Cache(CacheRequest),
    /// What's used of each of the account's limits now, shortest window first, when the adapter asked rather than
    /// the agent saying so by itself. It answers the adapter's own question, so it isn't an event, as `Context` isn't.
    Limits(Vec<UsageLimit>),
    /// Always the last thing sent.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
        stderr_tail: Vec<String>,
        requested: bool,
    },
}

impl AgentOutput {
    /// An event that names no images.
    pub fn event(body: EventBody, raw: Option<String>) -> Self {
        AgentOutput::Event { body, raw, attachments: Vec::new() }
    }
}

/// Names the Claude CLI directly, for tests and unusual installs.
pub use protocol::paths::ENV_CLAUDE_BIN;

/// How many milliseconds to wait for an agent's output once it has exited, in place of the usual few
/// seconds, so a test can do what it needs before that wait ends.
#[cfg(feature = "testing")]
pub const ENV_DRAIN_GRACE_MS: &str = "ROUNDHOUSE_DRAIN_GRACE_MS";

/// Where to look for an agent's CLI. `main` reads these from the environment.
#[derive(Clone, Debug, Default)]
pub struct BinaryLookup {
    /// Used as given, without looking anywhere else.
    pub explicit: Option<PathBuf>,
    /// The `PATH` to search, as the OS holds it.
    pub path: Option<OsString>,
    /// For the places the installers use under it.
    pub home: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct AgentOptions {
    pub binary: BinaryLookup,
    /// Records the running process, so something can stop it if this worker is killed. See
    /// `providers::local`. Removed when the process exits.
    pub pid_file: Option<PathBuf>,
    /// The worker's tools, which the agent is told about as it starts. `None` for a one-off call, and for an agent
    /// that goes without them.
    pub tools: Option<ToolServer>,
    /// What the agent is told about working in Roundhouse, added to its system prompt at every launch. `None` for a
    /// one-off call, which has a system prompt of its own.
    pub instructions: Option<String>,
    /// The login its hub holds for the account to run under, which the CLI is given for as long as it runs and nothing
    /// keeps afterwards. `None` runs it under this machine's own login.
    pub login: Option<AgentLogin>,
}

/// Where an agent reaches the worker's tools, over MCP.
#[derive(Clone, Debug)]
pub struct ToolServer {
    /// Such as `http://127.0.0.1:50123/mcp`.
    pub url: String,
    /// Says whose a call is, sent with every request.
    pub token: ToolsToken,
    /// Where the agent's CLI is told about them, written as it starts and removed once it has exited. It holds the
    /// token, so on Unix only its user can read it, and on Windows whoever its folder lets. Kept out of the worktree,
    /// where the agent and git would see it.
    pub config: PathBuf,
}

#[async_trait]
pub trait AgentProcess: Send + Sync {
    fn pid(&self) -> u32;
    /// The process has exited, or can't be reached. A new prompt needs a new process, even though
    /// its last output may still be on the way.
    fn is_gone(&self) -> bool;
    /// An error means this process can't take the prompt, and [`AgentProcess::is_gone`] is now true. The prompt can
    /// say nothing when it carries files, which are in the worktree already. `seq` is where the prompt is recorded,
    /// which the process names when it reports taking or dropping it.
    async fn send_prompt(&self, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError>;
    async fn resolve_approval(&self, approval_id: &ApprovalId, decision: &ApprovalDecision) -> Result<(), AgentError>;
    async fn interrupt(&self) -> Result<(), AgentError>;
    /// Asks the agent to compact its conversation, keeping what `keeping` says, in whatever way it's asked. `asked` is
    /// where that's recorded, which the process names when it reports taking or dropping it, and on the compaction.
    /// An error means what it does for [`AgentProcess::send_prompt`].
    async fn compact(&self, asked: Seq, keeping: Option<&str>) -> Result<(), AgentError>;
    /// Changes the model, approval mode or effort of the running process, and resolves once it has agreed. A setting
    /// that's `None` is left alone. An error with [`AgentProcess::is_gone`] false is a refusal, of the first setting
    /// refused, and the ones before it may already have been taken.
    async fn configure(&self, options: &SessionSettingsChange) -> Result<(), AgentError>;
    /// Asks the process to stop, and forces it if it doesn't. Resolves once it has exited.
    async fn stop(&self);
}

/// What each kind of agent this build runs is called and offers, as its adapter describes it, for the hub to give each
/// chat what its agent starts with and to tell clients what to offer.
pub fn descriptions() -> Vec<AgentDescription> {
    vec![claude::description()]
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    match spec.agent {
        AgentKind::Claude => claude::spawn(spec, output, options).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// The account `kind`'s CLI runs under here, with `options.login` or with this machine's own, from a one-off call
/// that runs in `scratch_dir`.
pub async fn account(kind: AgentKind, options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    match kind {
        AgentKind::Claude => claude::account(options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// A short title for a prompt, with the images it carried, from a one-off call to `kind`'s CLI that runs in an empty
/// directory made inside `scratch_dir`. `options.pid_file` records it while it runs.
pub async fn title_for(
    kind: AgentKind,
    prompt: &Prompt<NewAttachment>,
    options: &AgentOptions,
    scratch_dir: &Path,
) -> Result<Title, AgentError> {
    match kind {
        AgentKind::Claude => claude::title_for(prompt, options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}
