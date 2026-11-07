//! The session commands.

use std::io::Write;

use anyhow::bail;
use client_core::{Client, SubscriptionItem};
use protocol::event::EventBody;
use protocol::model::{AgentKind, Seq, Session, SessionStatus};

use crate::args::SessionCmd;
use crate::render;
use crate::run::short;
use crate::workspace::find_workspace;

pub(crate) fn session_command(client: &Client, command: SessionCmd) -> anyhow::Result<()> {
    match command {
        SessionCmd::New { workspace, model, permission_mode } => {
            let workspace = find_workspace(client, &workspace)?;
            let session = client.session_create(&workspace.id, AgentKind::Claude, model, permission_mode)?;
            println!("{}", session.id);
        }
        SessionCmd::List { workspace } => {
            let workspace = find_workspace(client, &workspace)?;
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
        SessionCmd::Set { session, model, permission_mode } => {
            let session = find_session(client, &session)?;
            let session = client.session_configure(&session.id, model, permission_mode)?;
            println!(
                "model={} mode={}",
                session.model.as_deref().unwrap_or("-"),
                session.permission_mode.as_deref().unwrap_or("-")
            );
        }
        SessionCmd::Close { session } => {
            let session = find_session(client, &session)?;
            client.session_close(&session.id)?;
        }
        SessionCmd::Reopen { session } => {
            let session = find_session(client, &session)?;
            client.session_reopen(&session.id)?;
        }
    }
    Ok(())
}

pub(crate) fn print_log(client: &Client, session: &str, from: Seq, limit: u32, raw: bool) -> anyhow::Result<()> {
    let session = find_session(client, session)?;
    let (events, _) = client.session_read(&session.id, from, limit, raw)?;
    let mut printer = render::Printer::default();
    for event in events {
        if raw {
            println!("{}", serde_json::to_string(&event)?);
        } else {
            printer.event(&event);
        }
    }
    Ok(())
}

/// A session ID or prefix, or a workspace name for that workspace's newest session.
pub(crate) fn find_session(client: &Client, query: &str) -> anyhow::Result<Session> {
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
            bail!("workspace {query} has no open sessions. Start one with `trestle session new {query}`");
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
pub(crate) fn follow(client: &Client, session_id: &str, from: Seq, until_idle: bool, json: bool) -> anyhow::Result<()> {
    let subscription = client.session_subscribe(session_id, from)?;
    let mut printer = render::Printer::default();
    let mut last_seq = from.saturating_sub(1);
    // Nothing is left to wait for if the agent has stopped and everything recorded has been seen.
    let finished = |seen: Seq| -> anyhow::Result<bool> {
        let session = client.session_get(session_id)?;
        Ok(session.last_seq <= seen && matches!(session.status, SessionStatus::Idle | SessionStatus::Interrupted))
    };
    for item in &subscription.items {
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
