//! Carrying out a command.

use std::path::PathBuf;

use anyhow::Context;
use client_core::{Client, Daemon, FileToSend, HubAdmin};
use protocol::event::ApprovalDecision;
use protocol::paths::Paths;
use protocol::versions::Versions;

use crate::args::{Args, Cmd, HubCmd, RepoCmd};
use crate::session::{find_session, follow, print_log, session_command};
use crate::workspace::workspace_command;

fn connect(paths: &Paths) -> anyhow::Result<Client> {
    let named = std::env::var_os(protocol::paths::ENV_DAEMON_BIN).map(PathBuf::from);
    let daemon = Daemon::find(named).context("finding the daemon next to this binary")?;
    Client::connect_or_start(paths, &daemon, "cli").context("connecting to the hub")
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
        Cmd::Send { session, prompt, no_follow, client_msg_id, attach } => {
            let session = find_session(&client, &session)?;
            let files = attach.iter().map(|path| FileToSend::read(path)).collect::<Result<Vec<_>, _>>()?;
            // Generated here rather than left to the hub, so a retry of this exact send is safe.
            let msg_id = client_msg_id.unwrap_or_else(client_core::new_message_id);
            let (seq, duplicate) = client.send_prompt_carrying(&session.id, &prompt, Some(msg_id), &files)?;
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
                Some(message) => ApprovalDecision::Deny { message: Some(message), interrupt: false },
                None => ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
            };
            client.session_approve(&session.id, &approval_id, decision)?;
        }
        Cmd::Answer { session, approval_id, question, answer } => {
            let session = find_session(&client, &session)?;
            let decision = ApprovalDecision::AnswerByText { answers: std::iter::once((question, answer)).collect() };
            client.session_approve(&session.id, &approval_id, decision)?;
        }
        Cmd::Cancel { session } => {
            let session = find_session(&client, &session)?;
            client.session_cancel(&session.id)?;
        }
        Cmd::Compact { session, keep } => {
            let session = find_session(&client, &session)?;
            client.session_compact(&session.id, keep.as_deref())?;
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

/// Through [`HubAdmin`], so these work on a hub of any version.
fn hub_command(paths: &Paths, command: &HubCmd) -> anyhow::Result<()> {
    let hub = match HubAdmin::connect(&paths.socket) {
        Ok(hub) => hub,
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
            let status = hub.status()?;
            println!(
                "pid {}  version {}  protocol {}  {} worker(s)  {} session(s) running, waits aside  {} client(s)  {}",
                status.pid,
                status.hub_version,
                status.protocol_version,
                status.workers,
                status.running_sessions,
                status.clients,
                if status.on_demand { "started on demand" } else { "started by hand" }
            );
            if status.keeping_awake {
                println!("keeping this machine awake while there's work");
            }
            if status.hub_version != protocol::BUILD_VERSION {
                println!("this client is version {}", protocol::BUILD_VERSION);
            }
            let speaks = Versions::from_hello(status.protocol_version, status.oldest_protocol_version);
            if !speaks.speaks(protocol::client::CLIENT_PROTOCOL_VERSION) {
                println!(
                    "this client speaks protocol {}, so it can only stop that hub",
                    protocol::client::CLIENT_PROTOCOL_VERSION
                );
            }
        }
        HubCmd::Stop => {
            hub.stop()?;
            println!("hub stopped");
        }
    }
    Ok(())
}

pub(crate) fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}
