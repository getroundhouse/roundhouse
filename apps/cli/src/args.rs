//! What the command line accepts.

use clap::{Parser, Subcommand};
use protocol::model::{ApprovalId, ClientMessageId, Seq};

#[derive(Parser)]
#[command(name = "trestle", version, about = "Run coding agents in parallel, each in its own worktree")]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Inspect or stop the hub. It starts by itself when a command needs it.
    #[command(subcommand)]
    Hub(HubCmd),
    #[command(subcommand)]
    Repo(RepoCmd),
    /// Workspaces: one worktree and branch per piece of work.
    #[command(subcommand, alias = "ws")]
    Workspace(WorkspaceCmd),
    #[command(subcommand)]
    Session(SessionCmd),
    /// Send a prompt, then follow until the agent stops working.
    Send {
        /// A session ID or prefix, or a workspace name for its newest session.
        session: String,
        prompt: String,
        /// Return as soon as the prompt is recorded.
        #[arg(long)]
        no_follow: bool,
        /// Makes retrying safe: the same ID is only ever recorded once.
        #[arg(long)]
        client_msg_id: Option<ClientMessageId>,
    },
    /// Follow a session's events from any point in its history.
    Attach {
        session: String,
        /// The first event to show. Defaults to the whole history.
        #[arg(long, default_value_t = 1)]
        from: Seq,
        /// Stop once the agent is no longer working.
        #[arg(long)]
        until_idle: bool,
        /// Print events as JSON, one per line, without following streaming text.
        #[arg(long)]
        json: bool,
    },
    /// Print a range of a session's events and exit.
    Log {
        session: String,
        #[arg(long, default_value_t = 1)]
        from: Seq,
        #[arg(long, default_value_t = 500)]
        limit: u32,
        /// Print each event as JSON, including the agent's original line.
        #[arg(long)]
        raw: bool,
    },
    /// Answer an approval the agent is waiting on.
    Approve {
        session: String,
        approval_id: ApprovalId,
        /// Refuse, with this message to the agent.
        #[arg(long)]
        deny: Option<String>,
    },
    /// Answer a question the agent asked.
    Answer { session: String, approval_id: ApprovalId, question: String, answer: String },
    /// Stop the agent's current turn. The session stays usable.
    Cancel { session: String },
}

#[derive(Subcommand)]
pub(crate) enum HubCmd {
    Status,
    Stop,
}

#[derive(Subcommand)]
pub(crate) enum RepoCmd {
    /// Register a local git repository.
    Add {
        path: String,
    },
    List,
}

#[derive(Subcommand)]
pub(crate) enum WorkspaceCmd {
    /// Create a worktree on a new branch.
    Create {
        /// A repository name or ID.
        repo: String,
        #[arg(long)]
        name: Option<String>,
        /// Branch from this ref. Defaults to the repository's default branch, fetched first.
        #[arg(long)]
        base: Option<String>,
    },
    List {
        /// Include archived workspaces.
        #[arg(long)]
        all: bool,
    },
    /// Remove the worktree and keep the branch and history.
    Archive {
        workspace: String,
        /// Archive even if that loses uncommitted changes or commits on no branch.
        #[arg(long)]
        force: bool,
    },
    /// Bring back an archived workspace's worktree.
    Restore {
        workspace: String,
    },
    Status {
        workspace: String,
    },
    /// Everything this branch changes against its base.
    Diff {
        workspace: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum SessionCmd {
    /// Start a session. Nothing runs until the first prompt.
    New {
        workspace: String,
        #[arg(long)]
        model: Option<String>,
        /// Passed to the agent, such as manual, plan, acceptEdits or bypassPermissions. Leave it
        /// out to use the mode from your own Claude settings.
        #[arg(long)]
        permission_mode: Option<String>,
    },
    List {
        workspace: String,
    },
    /// Change the model or approval mode, on the running agent too if there is one.
    Set {
        session: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        permission_mode: Option<String>,
    },
    /// Stop a session's agent and hide it. The history stays.
    Close {
        session: String,
    },
    Reopen {
        session: String,
    },
}
