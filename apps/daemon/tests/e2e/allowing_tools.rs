//! A hub that allows a tool's use itself when an agent with full access asks anyway, as its settings may say.

#![cfg(test)]

use std::path::{Path, PathBuf};

use client_core::{Connection, Subscription};
use protocol::capability::approvals::Decision;
use protocol::client::HubNotice;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{HubSettingsChange, Session, SessionId, SessionStatus};

use crate::support::{Stack, next_notice, wait_for_turn_end, workspace};

/// A command the agent asks about with full access, as Claude's CLI does, since it can't tell what it removes. It
/// removes the directory `scratch` names.
const UNCHECKED_REMOVAL: &str = "rm -rf \"$(cat scratch)\"";

fn allow_asked(on: bool) -> HubSettingsChange {
    HubSettingsChange { allow_asked_in_full_access: Some(on), ..HubSettingsChange::default() }
}

/// A directory in `worktree` for the removal to remove, named in `scratch`.
fn to_remove(worktree: &Path, name: &str) -> PathBuf {
    let dir = worktree.join(name);
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(worktree.join("scratch"), name).unwrap();
    dir
}

/// How a request was answered and whether the hub's setting answered it, or `None` where nothing did, and whether the
/// answer came next.
type Answered = (Option<(Option<Decision>, bool)>, bool);

/// Each request in `events`, in the order they're recorded, with how it was answered.
fn answered(events: &[SessionEvent]) -> Vec<Answered> {
    let answer = |at: usize, id| {
        events[at..].iter().enumerate().find_map(|(after, event)| match &event.body {
            EventBody::ApprovalResolved { approval_id, decision, by_setting } if approval_id == id => {
                Some(((decision.as_deref().cloned(), *by_setting), after == 1))
            }
            _ => None,
        })
    };
    let requests = events.iter().enumerate().filter_map(|(at, event)| match &event.body {
        EventBody::ApprovalRequested { approval_id, .. } => Some(answer(at, approval_id)),
        _ => None,
    });
    requests.map(|found| (found.clone().map(|(answer, _)| answer), found.is_some_and(|(_, next)| next))).collect()
}

/// When the person last did something, by the chat's record and by its workspace's, and when they last prompted it,
/// by its log.
fn last_active(client: &Connection, chat: &Session, events: &[SessionEvent]) -> [Option<i64>; 3] {
    let prompted = events.iter().rev().find(|event| matches!(event.body, EventBody::UserPrompt { .. }));
    [
        client.session_get(&chat.id).unwrap().user_active_at_ms,
        workspace(client, &chat.workspace_id).user_active_at_ms,
        prompted.map(|event| event.ts_ms),
    ]
}

/// Every copy of the chat's record the feed tells of from now until one says its turn has ended.
fn told_until_the_turn_ends(feed: &Subscription, chat: &SessionId) -> Vec<Session> {
    let (mut told, mut at_work) = (Vec::new(), false);
    loop {
        let HubNotice::Session(session) = next_notice(feed, "the chat's turn to end") else { continue };
        if session.id != *chat {
            continue;
        }
        at_work |= !session.status.between_turns();
        let ended = at_work && session.status.between_turns();
        told.push(session);
        if ended {
            return told;
        }
    }
}

/// Whether a copy of a chat's record says it's waiting on the person.
fn waits(session: &Session) -> bool {
    session.status == SessionStatus::WaitingApproval || !session.open_approvals.is_empty()
}

#[test]
fn a_hub_set_to_allow_it_allows_what_an_agent_with_full_access_asks_and_nobody_is_told_it_waits() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.unwrap());
    client.hub_configure(allow_asked(true)).unwrap();
    let removed = to_remove(&worktree, "first");
    let feed = client.follow(protocol::client::subscriptions::Follow::Hub).unwrap();

    client.session_prompt(&session.id, &format!("tool:{UNCHECKED_REMOVAL}"), None).unwrap();
    let told = told_until_the_turn_ends(&feed, &session.id);

    assert!(!removed.exists(), "it ran without anyone answering");
    let events = wait_for_turn_end(&client, &session.id, 1);
    let allowed = (Some((Some(protocol::fixtures::allow_once()), true)), true);
    assert_eq!(
        answered(&events),
        std::slice::from_ref(&allowed),
        "the log says it was asked, and allowed by the setting"
    );
    assert_eq!(told.iter().find(|session| waits(session)), None, "nobody is told it waits");
    let [chat, its_workspace, prompted] = last_active(&client, &session, &events);
    assert_eq!((chat, its_workspace), (prompted, prompted), "the person did nothing since they prompted it");

    // A subagent's request is the agent's own, so it's allowed the same.
    let removed = to_remove(&worktree, "second");
    client.session_prompt(&session.id, &format!("agents-ask:2:{UNCHECKED_REMOVAL}"), None).unwrap();
    let told = told_until_the_turn_ends(&feed, &session.id);

    assert!(!removed.exists());
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(answered(&events)[1..], [allowed.clone(), allowed]);
    assert_eq!(told.iter().find(|session| waits(session)), None);
}
