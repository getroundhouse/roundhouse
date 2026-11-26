use crate::ToolServers;
use process::Environment;
use protocol::model::AgentLogin;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

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
    /// The tool servers its worker serves, which the agent is told about as it starts. `None` for a one-off call, and
    /// for an agent that goes without them.
    pub tools: Option<ToolServers>,
    /// What the agent is told about working in Roundhouse, added to its system prompt at every launch. `None` for a
    /// one-off call, which has a system prompt of its own.
    pub instructions: Option<Instructions>,
    /// The login its hub holds for the account to run under, which the CLI is given for as long as it runs and nothing
    /// keeps afterwards. `None` runs it under this machine's own login.
    pub login: Option<AgentLogin>,
    /// What its worker gives every process it starts, such as the git credential its hub set, which goes on the CLI
    /// last as it starts.
    pub environment: Environment,
    /// How long the CLI has to let go of its output once it has exited.
    pub drain_grace: DrainGrace,
}

/// How long an agent's CLI that has exited has to let go of its output before its run ends without the rest. Something
/// it started can hold its output open for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrainGrace(pub Duration);

impl DrainGrace {
    /// Long enough for what the CLI printed as it exited to be read.
    pub const DEFAULT: Self = Self(Duration::from_secs(5));
}

impl Default for DrainGrace {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What an agent is told about working in Roundhouse, and where its CLI reads it from, if it reads it from a file.
#[derive(Clone, Debug)]
pub struct Instructions {
    pub text: String,
    /// Where a CLI that's given them in a file, as Claude's is, reads them, written at each launch and removed once the
    /// run ends. A file keeps them off the CLI's command line, where they'd fill every listing of its processes. Kept out
    /// of the worktree, where the agent and git would see it. Not private, since they're the same for every agent.
    pub file: PathBuf,
}
