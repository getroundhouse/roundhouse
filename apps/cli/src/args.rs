//! What the command line accepts.

use std::path::PathBuf;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Parser, Subcommand};
use protocol::model::{ApprovalId, ClientMessageId, Effort, ModelName, PermissionMode, Seq};

#[derive(Parser)]
#[command(name = "roundhouse", version = crate::BUILD.version(), about = "Run coding agents in parallel, each in its own worktree")]
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
    /// What each kind of agent the hub runs is offered: its models, efforts and approval modes, with what a chat starts
    /// with marked.
    Agents,
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
        /// A file to send with the prompt, of any kind, which the agent finds in its worktree. Can be given more
        /// than once, and with any, the prompt can be "".
        #[arg(long, value_name = "PATH")]
        attach: Vec<PathBuf>,
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
    /// Ask the agent to compact its conversation now, rather than when its context fills. Refused while it's on a
    /// turn.
    Compact {
        session: String,
        /// What to keep, in your own words.
        #[arg(long)]
        keep: Option<String>,
    },
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
    /// Change what it's called. Its name, which is its directory and branch, stays.
    Rename {
        workspace: String,
        title: String,
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
        /// One the agent is offered, as `roundhouse agents` lists them, or a full model ID. Leave it
        /// out for what the agent starts a chat with.
        #[arg(long, value_parser = model())]
        model: Option<ModelName>,
        /// plan, or bypassPermissions for full access, which is what leaving it out gives.
        #[arg(long, value_parser = mode())]
        permission_mode: Option<PermissionMode>,
        /// How hard the agent thinks. Leave it out for what the agent starts a chat with.
        #[arg(long, value_parser = effort())]
        effort: Option<Effort>,
    },
    List {
        workspace: String,
    },
    /// Change the model, approval mode or effort, on the running agent too if there is one.
    Set {
        session: String,
        #[arg(long, value_parser = model())]
        model: Option<ModelName>,
        /// plan, or bypassPermissions for full access.
        #[arg(long, value_parser = mode())]
        permission_mode: Option<PermissionMode>,
        #[arg(long, value_parser = effort())]
        effort: Option<Effort>,
    },
    /// Stop a session's agent and hide it. The history stays, and a session nothing was sent in is deleted.
    Close {
        session: String,
    },
    Reopen {
        session: String,
    },
    /// Change what it's called.
    Rename {
        session: String,
        title: String,
    },
}

/// Takes a name that names a model, so `default`, which would leave the choice to the agent's CLI, is refused here.
fn model() -> impl TypedValueParser<Value = ModelName> {
    clap::builder::StringValueParser::new().try_map(ModelName::try_from)
}

/// Takes the name of either mode a chat can have, and lists both in the help and in the error for another name.
fn mode() -> impl TypedValueParser<Value = PermissionMode> {
    PossibleValuesParser::new(PermissionMode::CHOSEN.into_iter().filter_map(PermissionMode::as_str))
        .try_map(|name| PermissionMode::parse(&name).ok_or("not a mode a chat can have"))
}

/// Takes the name of any level, and lists them all in the help and in the error for another name.
fn effort() -> impl TypedValueParser<Value = Effort> {
    PossibleValuesParser::new(Effort::ALL.into_iter().filter_map(Effort::as_str))
        .try_map(|name| Effort::parse(&name).ok_or("not a level of effort"))
}
