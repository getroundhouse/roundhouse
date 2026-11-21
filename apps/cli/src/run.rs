//! Carrying out a command.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::Context;
use client_core::{Approval, Client, FileToSend, HubAdmin, Written};
use local_hub::Daemon;
use protocol::capability::approvals::Decision;
use protocol::client::{HubStart, HubStatus, admin};
use protocol::event::SessionEvent;
use protocol::model::{ApprovalId, HubUpdate, Repo, RepoOrigin, Session, UpdateAttempt};
use protocol::paths::Paths;
use protocol::prompt::Prompt;
use protocol::versions::Versions;

use crate::agents::print_agents;
use crate::args::{Args, Cmd, HubCmd, RepoCmd};
use crate::session::{find_session, follow, print_log, session_command};
use crate::workspace::workspace_command;

fn connect(paths: &Paths) -> anyhow::Result<Client> {
    let named = std::env::var_os(protocol::paths::ENV_DAEMON_BIN).map(PathBuf::from);
    let daemon = Daemon::find(named, crate::BUILD).context("finding the daemon next to this binary")?;
    local_hub::connect_or_start(paths, &daemon, "cli").context("connecting to the hub")
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
        Cmd::Agents => print_agents(&client)?,
        Cmd::Phone(command) => crate::phone::phone_command(&client, command)?,
        Cmd::Send { session, prompt, no_follow, client_msg_id, attach } => {
            let session = find_session(&client, &session)?;
            let files = attach.iter().map(|path| FileToSend::read(path)).collect::<Result<Vec<_>, _>>()?;
            // Generated here rather than left to the hub, so a retry of this exact send is safe.
            let msg_id = client_msg_id.unwrap_or_else(client_core::new_message_id);
            let written = Written { client_msg_id: msg_id, prompt: Prompt::carrying(prompt, files) };
            let (seq, duplicate) = client.send_prompt(&session.id, &written)?;
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
            let decision = approving(&client, &session, &approval_id, deny)?;
            client.session_decide(&session.id, &approval_id, decision)?;
        }
        Cmd::Answer { session, approval_id, question, answer } => {
            let session = find_session(&client, &session)?;
            let asked = waiting(&client, &session, &approval_id)?;
            let approval = Approval::of(&asked.body).context("not a request")?;
            let decision = approval
                .answered_in_words(&question, &answer)
                .with_context(|| format!("approval {approval_id} asks no question {question:?}"))?;
            client.session_decide(&session.id, &approval_id, decision)?;
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

/// The request waiting in `session` under `approval`.
fn waiting(client: &Client, session: &Session, approval: &ApprovalId) -> anyhow::Result<SessionEvent> {
    client.waiting_request(session, approval)?.with_context(|| format!("no approval {approval} is waiting"))
}

/// What `roundhouse approve` decides of the request waiting under `approval`: a call allowed this once, or a plan
/// approved, or either refused in the words `deny`.
fn approving(
    client: &Client,
    session: &Session,
    approval: &ApprovalId,
    deny: Option<String>,
) -> anyhow::Result<Decision> {
    let asked = waiting(client, session, approval)?;
    let request = Approval::of(&asked.body).context("not a request")?;
    if request.questions().is_some() {
        anyhow::bail!("approval {approval} asks questions: answer them with answer");
    }
    if deny.is_some() {
        return Ok(request.refuse(deny));
    }
    request.approve().with_context(|| format!("approval {approval} asks for more than a yes"))
}

fn repo_command(client: &Client, command: RepoCmd) -> anyhow::Result<()> {
    match command {
        RepoCmd::Add { path } => {
            let absolute = std::path::absolute(&path).with_context(|| format!("resolving {path}"))?;
            let repo = client.repo_add(&absolute.to_string_lossy())?;
            println!("{}  {}", repo.name, place(&repo));
        }
        RepoCmd::List => {
            for repo in client.repo_list()? {
                println!("{}  {}  {}", short(repo.id.as_str()), repo.name, place(&repo));
            }
        }
    }
    Ok(())
}

/// Where a repository is, as the repository commands print it: its checkout, as every repository's was before
/// addresses, or the address of one with no checkout on the hub's machine.
fn place(repo: &Repo) -> String {
    match &repo.origin {
        RepoOrigin::Checkout { path } | RepoOrigin::Linked { path, .. } => path.clone(),
        RepoOrigin::Remote { url } => url.to_string(),
        RepoOrigin::Unknown => "somewhere this build can't say".to_owned(),
    }
}

fn hub_command(paths: &Paths, command: &HubCmd) -> anyhow::Result<()> {
    match command {
        HubCmd::Status => admin_command(paths, Admin::Status),
        HubCmd::Stop => admin_command(paths, Admin::Stop),
        HubCmd::Update => admin_command(paths, Admin::Update),
        HubCmd::Install { user } => crate::service::install(user),
        HubCmd::Uninstall => crate::service::uninstall(),
        HubCmd::Place => crate::service::place(paths),
    }
}

/// What a `hub` command asks of the running hub.
#[derive(Clone, Copy)]
enum Admin {
    Status,
    Stop,
    Update,
}

/// Through [`HubAdmin`], so these work on a hub of any version.
/// What a hub installed as a service last found in the update feed, and how an update it was asked for went, as `hub
/// status` says it.
fn updates_line(update: Option<&HubUpdate>) -> String {
    let Some(update) = update else { return "it hasn't read the update feed yet".to_owned() };
    if let Some(UpdateAttempt::UnderWay { to }) = &update.attempt {
        return format!("it's updating to {to}");
    }
    let mut line = match &update.version {
        Some(version) => format!("it would update to {version}"),
        None => "no newer release it can update to".to_owned(),
    };
    if let Some(problem) = &update.problem {
        let _ = write!(line, ", though its last read of the update feed failed: {problem}");
    }
    if let Some(UpdateAttempt::Failed { to, why }) = &update.attempt {
        let _ = write!(line, ". Its update to {to} failed: {why}");
    }
    line
}

fn admin_command(paths: &Paths, command: Admin) -> anyhow::Result<()> {
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
        Admin::Status => {
            let status = hub.status()?;
            println!(
                "pid {}  version {}  protocol {}  {} worker(s)  {} session(s) running, waits aside  {} client(s)  {}",
                status.pid,
                status.hub_version,
                status.protocol_version,
                status.workers,
                status.running_sessions,
                status.clients,
                started(status.start()),
            );
            if status.keeping_awake {
                println!("keeping this machine awake while there's work");
            }
            if status.hub_version != crate::BUILD.version() {
                println!("this client is version {}", crate::BUILD.version());
            }
            let speaks = Versions::from_hello(status.protocol_version, status.oldest_protocol_version);
            if !speaks.speaks(protocol::client::CLIENT_PROTOCOL_VERSION) {
                println!(
                    "this client speaks protocol {}, so it can only stop that hub",
                    protocol::client::CLIENT_PROTOCOL_VERSION
                );
            } else if status.start() == HubStart::AsService {
                // The hub is running, so this connects to it rather than starting one.
                let records = connect(paths)?.records_list()?;
                println!("{}", updates_line(records.hub_update.as_ref()));
            }
        }
        Admin::Update => update_hub(paths, &hub.status()?)?,
        Admin::Stop => {
            // Asked first, since a hub that has stopped can't say.
            let agents = hub.status()?.running_sessions;
            hub.stop()?;
            println!("{}", stopped(agents));
        }
    }
    Ok(())
}

/// How a hub was started, as `hub status` says it.
fn started(start: HubStart) -> &'static str {
    match start {
        HubStart::ByHand => "started by hand",
        HubStart::OnDemand => "started on demand",
        HubStart::AsService => "installed as a service",
    }
}

/// Has the hub that's running, with `status`, update to the release it found, if it's installed as a service, which is
/// the only kind that reads the feed.
fn update_hub(paths: &Paths, status: &HubStatus) -> anyhow::Result<()> {
    if status.start() != HubStart::AsService {
        println!("only a hub installed as a service updates from here, and this one was {}", started(status.start()));
        return Ok(());
    }
    // The hub is running, so this connects to it rather than starting one.
    let client = connect(paths)?;
    let found = client.records_list()?.hub_update;
    let Some(version) = found.as_ref().and_then(|update| update.version.clone()) else {
        println!("{}", updates_line(found.as_ref()));
        return Ok(());
    };
    match client.call(admin::HubUpdateTo { version: version.clone() }) {
        Err(e) if e.is_unsupported() => {
            println!(
                "that hub is from before a client could update it, so install a newer release's hub on its machine"
            );
        }
        updating => {
            updating?;
            println!(
                "the hub is updating to {version}, and restarts on it once it's placed, with its agents still at work"
            );
        }
    }
    Ok(())
}

/// What `hub stop` says, with this many agents at work on the hub as it stopped, which it stopped with it. One only
/// waiting on its pull request isn't, since the next hub keeps the wait.
fn stopped(agents: u32) -> String {
    match agents {
        0 => "hub stopped".to_string(),
        1 => "hub stopped, with the 1 agent at work on it, which picks up its conversation at its next prompt"
            .to_string(),
        n => format!(
            "hub stopped, with the {n} agents at work on it, which pick up their conversations at their next prompts"
        ),
    }
}

pub(crate) fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

#[cfg(test)]
mod tests {
    use protocol::model::{HubUpdate, Rev, UpdateAttempt};

    use super::{stopped, updates_line};

    #[test]
    fn stopping_the_hub_says_how_many_agents_it_stopped() {
        assert_eq!(stopped(0), "hub stopped");
        assert!(
            stopped(1).starts_with("hub stopped, with the 1 agent at work on it, which picks up"),
            "{}",
            stopped(1)
        );
        assert!(stopped(3).starts_with("hub stopped, with the 3 agents at work on it"), "{}", stopped(3));
    }

    #[test]
    fn a_service_s_status_says_what_it_found_in_the_update_feed() {
        let found = |version: Option<&str>, problem: Option<&str>| HubUpdate {
            version: version.map(str::to_owned),
            looked_at_ms: 1,
            problem: problem.map(str::to_owned),
            attempt: None,
            rev: Rev::default(),
        };
        let attempt = |attempt| HubUpdate { attempt: Some(attempt), ..found(Some("0.1.9"), None) };
        let failed = UpdateAttempt::Failed { to: "0.1.9".into(), why: "offline".into() };

        assert_eq!(updates_line(None), "it hasn't read the update feed yet");
        assert_eq!(updates_line(Some(&found(Some("0.1.9"), None))), "it would update to 0.1.9");
        assert_eq!(updates_line(Some(&found(None, None))), "no newer release it can update to");
        assert_eq!(
            updates_line(Some(&found(Some("0.1.9"), Some("offline")))),
            "it would update to 0.1.9, though its last read of the update feed failed: offline"
        );
        assert_eq!(
            updates_line(Some(&attempt(UpdateAttempt::UnderWay { to: "0.1.9".into() }))),
            "it's updating to 0.1.9"
        );
        assert_eq!(
            updates_line(Some(&attempt(failed))),
            "it would update to 0.1.9. Its update to 0.1.9 failed: offline"
        );
    }
}
