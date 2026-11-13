//! The session commands.

use std::io::Write;

use anyhow::bail;
use client_core::{Agents, Client, Ended, SubscriptionItem};
use protocol::event::EventBody;
use protocol::model::{Effort, PermissionMode, Seq, Session, SessionId, SessionSettingsChange};

use crate::args::SessionCmd;
use crate::render;
use crate::run::short;
use crate::workspace::find_workspace;

pub(crate) fn session_command(client: &Client, command: SessionCmd) -> anyhow::Result<()> {
    match command {
        SessionCmd::New { workspace, model, permission_mode, effort } => {
            let workspace = find_workspace(client, &workspace)?;
            let options = SessionSettingsChange { model, permission_mode, effort };
            let agent = Agents::read(client)?.for_new_chat();
            let session = client.session_create(&workspace.id, agent, options, Some(client_core::new_request_id()))?;
            println!("{}", session.id);
        }
        SessionCmd::List { workspace } => {
            let workspace = find_workspace(client, &workspace)?;
            for session in client.session_list(&workspace.id)? {
                let closed = if session.is_open() { "" } else { "  closed" };
                let unread = if session.unread() { "  unread" } else { "" };
                let failed = if session.turn_failed() { "  turn failed" } else { "" };
                println!(
                    "{}  {:?}{closed}{unread}{failed}  {} events  {}  {}",
                    short(session.id.as_str()),
                    session.status,
                    session.last_seq,
                    options(&session),
                    session.title.as_deref().unwrap_or(""),
                );
            }
        }
        SessionCmd::Set { session, model, permission_mode, effort } => {
            let session = find_session(client, &session)?;
            let session =
                client.session_configure(&session.id, SessionSettingsChange { model, permission_mode, effort })?;
            println!("{}", options(&session));
        }
        SessionCmd::Close { session } => {
            let session = find_session(client, &session)?;
            client.session_close(&session.id)?;
        }
        SessionCmd::Reopen { session } => {
            let session = find_session(client, &session)?;
            client.session_reopen(&session.id)?;
        }
        SessionCmd::Rename { session, title } => {
            let session = find_session(client, &session)?;
            let renamed = client.session_rename(&session.id, &title)?;
            println!("renamed {} to {}", short(renamed.id.as_str()), renamed.title.as_deref().unwrap_or_default());
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

/// What the session runs with.
fn options(session: &Session) -> String {
    format!(
        "model={} mode={} effort={}",
        session.model.as_str(),
        mode_name(session.permission_mode),
        effort_name(session.effort),
    )
}

/// A mode an agent went into by itself, or one from a newer hub, has no name here.
pub(crate) fn mode_name(mode: PermissionMode) -> &'static str {
    mode.as_str().unwrap_or("unknown")
}

/// A level from a newer hub has no name here.
pub(crate) fn effort_name(effort: Effort) -> &'static str {
    effort.as_str().unwrap_or("unknown")
}

/// A session ID or prefix, or a workspace name for that workspace's newest session.
pub(crate) fn find_session(client: &Client, query: &str) -> anyhow::Result<Session> {
    if let Ok(session) = client.session_get(&query.into()) {
        return Ok(session);
    }
    let workspaces = client.workspace_list(true)?;
    let mut by_prefix = Vec::new();
    for workspace in &workspaces {
        let sessions = client.session_list(&workspace.id)?;
        if workspace.name == query {
            if let Some(newest) = sessions.iter().rfind(|s| s.is_open()) {
                return Ok(newest.clone());
            }
            bail!("workspace {query} has no open sessions. Start one with `roundhouse session new {query}`");
        }
        by_prefix.extend(sessions.into_iter().filter(|s| s.id.as_str().starts_with(query)));
    }
    match by_prefix.len() {
        0 => bail!("no session matches {query}"),
        1 => Ok(by_prefix.remove(0)),
        n => bail!("{query} matches {n} sessions. Use more of the ID"),
    }
}

/// Prints a session from `from` on, and keeps following until told otherwise. Exits if the hub goes
/// away. Running it again with `--from` picks up where it stopped.
pub(crate) fn follow(
    client: &Client,
    session_id: &SessionId,
    from: Seq,
    until_idle: bool,
    json: bool,
) -> anyhow::Result<()> {
    let mut printer = render::Printer::default();
    let mut following = Following::starting_at(from);
    // Nothing is left to wait for if the agent is done and everything recorded has been seen. A status from a newer
    // hub doesn't say it's done, so following goes on.
    let finished = |following: Following| -> anyhow::Result<bool> {
        let session = client.session_get(session_id)?;
        Ok(following.has_all(session.last_seq) && session.status.is_done())
    };
    loop {
        let subscription = client.session_subscribe(session_id, following.next())?;
        for item in &subscription.items {
            let check = match item {
                SubscriptionItem::Event(event) => {
                    following.saw(event.seq);
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
            };
            if until_idle && check && finished(following)? {
                return Ok(());
            }
        }
        // Printing fell behind the agent, so it carries on from the last event printed.
        if subscription.ended() != Some(Ended::FellBehind) {
            std::io::stdout().flush().ok();
            let next = following.next();
            bail!("lost the hub before event {next}. Resume with `attach --from {next}`");
        }
    }
}

/// Where following a session's log has got to: the event to print next, as the hub's own subscription keeps it.
#[derive(Clone, Copy)]
struct Following {
    next: Seq,
}

impl Following {
    /// From `from`, or from the first event for 0, which is before it, as the hub reads it. From 0, nothing would have
    /// every event until one was printed, since no `last_seq` is before 0, so `--until-idle` on a chat with none would
    /// wait for good.
    fn starting_at(from: Seq) -> Self {
        Self { next: from.max(Seq::FIRST) }
    }

    /// The event at `seq` has been printed.
    fn saw(&mut self, seq: Seq) {
        self.next = seq.next();
    }

    /// The event to print next, which is where to follow from, and to say to resume from.
    fn next(self) -> Seq {
        self.next
    }

    /// Whether every event of a session whose newest is `last_seq` has been printed.
    fn has_all(self, last_seq: Seq) -> bool {
        last_seq < self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_from_a_newer_hub_is_shown_as_unknown_rather_than_as_none() {
        let shown = [Effort::Max, Effort::Unknown].map(effort_name);

        assert_eq!(shown, ["max", "unknown"]);
    }

    #[test]
    fn a_mode_a_chat_cant_have_is_shown_as_unknown() {
        let shown = [PermissionMode::Plan, PermissionMode::Unknown].map(mode_name);

        assert_eq!(shown, ["plan", "unknown"]);
    }

    #[test]
    fn following_from_0_has_every_event_of_a_chat_with_none() {
        let following = Following::starting_at(Seq::default());

        assert!(following.has_all(Seq::default()));
        assert_eq!(following.next(), Seq::FIRST, "and asks from the first");
    }

    #[test]
    fn following_has_every_event_once_it_has_printed_the_newest() {
        let mut following = Following::starting_at(Seq::FIRST);

        following.saw(Seq::new(7));

        assert!(following.has_all(Seq::new(7)));
        assert!(!following.has_all(Seq::new(8)), "not once another is recorded");
        assert_eq!(following.next(), Seq::new(8));
    }

    #[test]
    fn following_again_from_where_it_stopped_asks_for_that_event_first() {
        let following = Following::starting_at(Seq::new(8));

        assert_eq!(following.next(), Seq::new(8));
        assert!(following.has_all(Seq::new(7)), "having printed everything before it last time");
        assert!(!following.has_all(Seq::new(8)));
    }
}
