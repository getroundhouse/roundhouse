use protocol::model::AgentLogin;
use protocol::worker::ToolsToken;
use std::ffi::OsString;
use std::path::PathBuf;

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
