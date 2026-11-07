//! Command-line client. It only talks to the hub, the same way the desktop app will.

mod render;

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, anyhow, bail};
use clap::{Parser, Subcommand};
use client_core::{Client, SubscriptionItem};
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{AgentKind, Seq, Session, SessionStatus, Workspace};
use protocol::paths::Paths;

#[derive(Parser)]
#[command(name = "agenthub", version, about = "Run coding agents in parallel, each in its own worktree")]
struct Args {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
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
        client_msg_id: Option<String>,
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
        approval_id: String,
        /// Refuse, with this message to the agent.
        #[arg(long)]
        deny: Option<String>,
    },
    /// Answer a question the agent asked.
    Answer { session: String, approval_id: String, question: String, answer: String },
    /// Stop the agent's current turn. The session stays usable.
    Cancel { session: String },
}

#[derive(Subcommand)]
enum HubCmd {
    Status,
    Stop,
}

#[derive(Subcommand)]
enum RepoCmd {
    /// Register a local git repository.
    Add {
        path: String,
    },
    List,
}

#[derive(Subcommand)]
enum WorkspaceCmd {
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
        /// Archive even with uncommitted changes, which are lost.
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
enum SessionCmd {
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

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn connect(paths: &Paths) -> anyhow::Result<Client> {
    let daemon = find_daemon()?;
    Client::connect_or_start(paths, &daemon, "cli").context("connecting to the hub")
}

/// The daemon ships next to this binary.
fn find_daemon() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("AGENTHUB_DAEMON_BIN") {
        return Ok(path.into());
    }
    let exe = std::env::current_exe().context("finding this binary")?;
    Ok(exe.with_file_name("agenthubd"))
}

fn run(args: Args) -> anyhow::Result<()> {
    let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
    // Don't start a hub just to report that none is running, or to stop it.
    if let Cmd::Hub(command) = &args.command {
        return hub_command(&paths, command);
    }
    let client = connect(&paths)?;
    match args.command {
        Cmd::Hub(_) => unreachable!("handled above"),
        Cmd::Repo(RepoCmd::Add { path }) => {
            let absolute = std::path::absolute(&path).with_context(|| format!("resolving {path}"))?;
            let repo = client.repo_add(&absolute.to_string_lossy())?;
            println!("{}  {}", repo.name, repo.path);
        }
        Cmd::Repo(RepoCmd::List) => {
            for repo in client.repo_list()? {
                println!("{}  {}  {}", short(&repo.id), repo.name, repo.path);
            }
        }
        Cmd::Workspace(command) => workspace_command(&client, command)?,
        Cmd::Session(SessionCmd::New { workspace, model, permission_mode }) => {
            let workspace = find_workspace(&client, &workspace)?;
            let session = client.session_create(&workspace.id, AgentKind::Claude, model, permission_mode)?;
            println!("{}", session.id);
        }
        Cmd::Session(SessionCmd::List { workspace }) => {
            let workspace = find_workspace(&client, &workspace)?;
            for session in client.session_list(&workspace.id)? {
                let closed = if session.closed_at_ms.is_some() { "  closed" } else { "" };
                println!(
                    "{}  {:?}{closed}  {} events  model={} mode={}  {}",
                    short(&session.id),
                    session.status,
                    session.last_seq,
                    session.model.as_deref().unwrap_or("-"),
                    session.permission_mode.as_deref().unwrap_or("-"),
                    session.title.as_deref().unwrap_or(""),
                );
            }
        }
        Cmd::Session(SessionCmd::Set { session, model, permission_mode }) => {
            let session = find_session(&client, &session)?;
            let session = client.session_configure(&session.id, model, permission_mode)?;
            println!(
                "model={} mode={}",
                session.model.as_deref().unwrap_or("-"),
                session.permission_mode.as_deref().unwrap_or("-")
            );
        }
        Cmd::Session(SessionCmd::Close { session }) => {
            let session = find_session(&client, &session)?;
            client.session_close(&session.id)?;
        }
        Cmd::Session(SessionCmd::Reopen { session }) => {
            let session = find_session(&client, &session)?;
            client.session_reopen(&session.id)?;
        }
        Cmd::Send { session, prompt, no_follow, client_msg_id } => {
            let session = find_session(&client, &session)?;
            // Generated here rather than left to the hub, so a retry of this exact send is safe.
            let msg_id = client_msg_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let (seq, duplicate) = client.session_prompt(&session.id, &prompt, Some(msg_id))?;
            if duplicate {
                eprintln!("(already recorded as event {seq})");
            }
            if !no_follow {
                follow(&client, &session.id, seq, true, false)?;
            }
        }
        Cmd::Attach { session, from, until_idle, json } => {
            let session = find_session(&client, &session)?;
            follow(&client, &session.id, from, until_idle, json)?;
        }
        Cmd::Log { session, from, limit, raw } => {
            let session = find_session(&client, &session)?;
            let (events, _) = client.session_read(&session.id, from, limit, raw)?;
            let mut printer = render::Printer::default();
            for event in events {
                if raw {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    printer.event(&event);
                }
            }
        }
        Cmd::Approve { session, approval_id, deny } => {
            let session = find_session(&client, &session)?;
            let decision = match deny {
                Some(message) => ApprovalDecision::Deny { message, interrupt: false },
                None => ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
            };
            client.session_approve(&session.id, &approval_id, decision)?;
        }
        Cmd::Answer { session, approval_id, question, answer } => {
            let session = find_session(&client, &session)?;
            let decision = ApprovalDecision::Answer { answers: std::iter::once((question, answer)).collect() };
            client.session_approve(&session.id, &approval_id, decision)?;
        }
        Cmd::Cancel { session } => {
            let session = find_session(&client, &session)?;
            client.session_cancel(&session.id)?;
        }
    }
    Ok(())
}

fn hub_command(paths: &Paths, command: &HubCmd) -> anyhow::Result<()> {
    let client = match Client::connect(&paths.socket, "cli") {
        Ok(client) => client,
        Err(client_core::ClientError::Io(e))
            if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) =>
        {
            println!("no hub is running");
            return Ok(());
        }
        Err(e) => return Err(e).context("connecting to the hub"),
    };
    match command {
        HubCmd::Status => {
            let status = client.hub_status()?;
            println!(
                "pid {}  version {}  protocol {}  {} worker(s)  {} session(s) running  {} client(s)",
                status.pid,
                status.hub_version,
                status.protocol_version,
                status.workers,
                status.running_sessions,
                status.clients
            );
        }
        HubCmd::Stop => {
            client.hub_shutdown()?;
            println!("hub stopping");
        }
    }
    Ok(())
}

fn workspace_command(client: &Client, command: WorkspaceCmd) -> anyhow::Result<()> {
    match command {
        WorkspaceCmd::Create { repo, name, base } => {
            let repos = client.repo_list()?;
            let found = repos
                .iter()
                .find(|r| r.name == repo || r.id == repo)
                .or_else(|| repos.iter().find(|r| r.id.starts_with(&repo)))
                .ok_or_else(|| anyhow!("no repository matches {repo}. Add one with `agenthub repo add <path>`"))?;
            let workspace = client.workspace_create(&found.id, name, base)?;
            println!("{}", workspace.name);
            println!("  path    {}", workspace.path.unwrap_or_default());
            println!("  branch  {}", workspace.branch);
            println!("  base    {}", workspace.base_ref.unwrap_or_default());
        }
        WorkspaceCmd::List { all } => {
            for workspace in client.workspace_list(all)? {
                println!(
                    "{}  {}  {:?}  {}",
                    short(&workspace.id),
                    workspace.name,
                    workspace.status,
                    workspace.path.unwrap_or_default()
                );
            }
        }
        WorkspaceCmd::Archive { workspace, force } => {
            let found = find_workspace(client, &workspace)?;
            client.workspace_archive(&found.id, force)?;
            println!("archived {}. Branch {} was kept", found.name, found.branch);
        }
        WorkspaceCmd::Restore { workspace } => {
            let found = find_workspace(client, &workspace)?;
            let restored = client.workspace_restore(&found.id)?;
            println!("restored {} at {}", restored.name, restored.path.unwrap_or_default());
        }
        WorkspaceCmd::Status { workspace } => {
            let found = find_workspace(client, &workspace)?;
            let status = client.workspace_status(&found.id)?;
            println!("on {}", status.branch.as_deref().unwrap_or("detached HEAD"));
            for file in status.files {
                println!("{}  {}", file.status, file.path);
            }
        }
        WorkspaceCmd::Diff { workspace } => {
            let found = find_workspace(client, &workspace)?;
            let diff = client.workspace_diff(&found.id)?;
            print!("{}", diff.patch);
            if diff.truncated {
                eprintln!("(some untracked files were left out)");
            }
        }
    }
    Ok(())
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

fn find_workspace(client: &Client, query: &str) -> anyhow::Result<Workspace> {
    let workspaces = client.workspace_list(true)?;
    let mut matches: Vec<_> = workspaces.iter().filter(|w| w.name == query || w.id == query).collect();
    if matches.is_empty() {
        matches = workspaces.iter().filter(|w| w.id.starts_with(query)).collect();
    }
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("no workspace matches {query}"),
        many => bail!("{query} matches {} workspaces. Use more of the ID", many.len()),
    }
}

/// A session ID or prefix, or a workspace name for that workspace's newest session.
fn find_session(client: &Client, query: &str) -> anyhow::Result<Session> {
    if let Ok(session) = client.session_get(query) {
        return Ok(session);
    }
    let workspaces = client.workspace_list(true)?;
    let mut by_prefix = Vec::new();
    for workspace in &workspaces {
        let sessions = client.session_list(&workspace.id)?;
        if workspace.name == query {
            if let Some(newest) = sessions.iter().rfind(|s| s.closed_at_ms.is_none()) {
                return Ok(newest.clone());
            }
            bail!("workspace {query} has no open sessions. Start one with `agenthub session new {query}`");
        }
        by_prefix.extend(sessions.into_iter().filter(|s| s.id.starts_with(query)));
    }
    match by_prefix.len() {
        0 => bail!("no session matches {query}"),
        1 => Ok(by_prefix.remove(0)),
        n => bail!("{query} matches {n} sessions. Use more of the ID"),
    }
}

/// Prints a session from `from` on, and keeps following until told otherwise. Exits if the hub goes
/// away. Running it again with `--from` picks up where it stopped.
fn follow(client: &Client, session_id: &str, from: Seq, until_idle: bool, json: bool) -> anyhow::Result<()> {
    let subscription = client.session_subscribe(session_id, from)?;
    let mut printer = render::Printer::default();
    let mut last_seq = from.saturating_sub(1);
    // Nothing is left to wait for if the agent has stopped and everything recorded has been seen.
    let finished = |seen: Seq| -> anyhow::Result<bool> {
        let session = client.session_get(session_id)?;
        Ok(session.last_seq <= seen && matches!(session.status, SessionStatus::Idle | SessionStatus::Interrupted))
    };
    for item in subscription.items.iter() {
        let check = match item {
            SubscriptionItem::Event(event) => {
                last_seq = event.seq;
                if json {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    printer.event(&event);
                }
                matches!(
                    event.body,
                    EventBody::AgentState { .. } | EventBody::TurnEnded { .. } | EventBody::AgentExited { .. }
                )
            }
            SubscriptionItem::Delta(delta) => {
                if !json {
                    printer.delta(&delta);
                }
                false
            }
            // A tail arrives once the subscription has caught up with the log.
            SubscriptionItem::Tail(tail) => {
                if !json {
                    printer.tail(&tail);
                }
                true
            }
            SubscriptionItem::Notice(_) => false,
            SubscriptionItem::Disconnected => {
                std::io::stdout().flush().ok();
                bail!("lost the hub after event {last_seq}. Resume with `attach --from {}`", last_seq + 1);
            }
        };
        if until_idle && check && finished(last_seq)? {
            return Ok(());
        }
    }
    Ok(())
}
