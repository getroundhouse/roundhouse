//! Carrying out a command.

use std::path::PathBuf;

use anyhow::Context;
use client_core::Client;
use protocol::event::ApprovalDecision;
use protocol::paths::Paths;

use crate::args::{Args, Cmd, HubCmd, RepoCmd};
use crate::session::{find_session, follow, print_log, session_command};
use crate::workspace::workspace_command;

fn connect(paths: &Paths) -> anyhow::Result<Client> {
    let daemon = find_daemon()?;
    Client::connect_or_start(paths, &daemon, "cli").context("connecting to the hub")
}

/// The daemon ships next to this binary.
fn find_daemon() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("TRESTLE_DAEMON_BIN") {
        return Ok(path.into());
    }
    let exe = std::env::current_exe().context("finding this binary")?;
    Ok(exe.with_file_name("trestled"))
}

pub(crate) fn run(args: Args) -> anyhow::Result<()> {
    let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
    // Don't start a hub just to report that none is running, or to stop it.
    if let Cmd::Hub(command) = &args.command {
        return hub_command(&paths, command);
    }
    let client = connect(&paths)?;
    match args.command {
        Cmd::Hub(_) => unreachable!("handled above"),
        Cmd::Repo(command) => repo_command(&client, command)?,
        Cmd::Workspace(command) => workspace_command(&client, command)?,
        Cmd::Session(command) => session_command(&client, command)?,
        Cmd::Send { session, prompt, no_follow, client_msg_id } => {
            let session = find_session(&client, &session)?;
            // Generated here rather than left to the hub, so a retry of this exact send is safe.
            let msg_id = client_msg_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string().into());
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
        Cmd::Log { session, from, limit, raw } => print_log(&client, &session, from, limit, raw)?,
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

fn repo_command(client: &Client, command: RepoCmd) -> anyhow::Result<()> {
    match command {
        RepoCmd::Add { path } => {
            let absolute = std::path::absolute(&path).with_context(|| format!("resolving {path}"))?;
            let repo = client.repo_add(&absolute.to_string_lossy())?;
            println!("{}  {}", repo.name, repo.path);
        }
        RepoCmd::List => {
            for repo in client.repo_list()? {
                println!("{}  {}  {}", short(repo.id.as_str()), repo.name, repo.path);
            }
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

pub(crate) fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}
