//! A hub that allows a tool's use itself when an agent with full access asks anyway, as its settings may say.

#![cfg(test)]

use std::path::{Path, PathBuf};

use client_core::{Client, Subscription};
use protocol::client::HubNotice;
use protocol::event::{ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{AgentKind, HubSettingsChange, PermissionMode, Session, SessionId, SessionStatus};

use crate::support::{Stack, in_mode, next_notice, wait_for_approval, wait_for_status, wait_for_turn_end, workspace};

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

/// Each request in `events`, in the order they're recorded, with how it was answered and whether the hub's setting
/// answered it, and whether the answer came next.
fn answered(events: &[SessionEvent]) -> Vec<(Option<(ApprovalOutcome, bool)>, bool)> {
    let answer = |at: usize, id| {
        events[at..].iter().enumerate().find_map(|(after, event)| match &event.body {
            EventBody::ApprovalResolved { approval_id, outcome, by_setting, .. } if approval_id == id => {
                Some(((*outcome, *by_setting), after == 1))
            }
            _ => None,
        })
    };
    let requests = events.iter().enumerate().filter_map(|(at, event)| match &event.body {
        EventBody::ApprovalRequested { approval_id, .. } => Some(answer(at, approval_id)),
        _ => None,
    });
    requests.map(|found| (found.map(|(answer, _)| answer), found.is_some_and(|(_, next)| next))).collect()
}

/// When the person last did something, by the chat's record and by its workspace's, and when they last prompted it,
/// by its log.
fn last_active(client: &Client, chat: &Session, events: &[SessionEvent]) -> [Option<i64>; 3] {
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
    session.status == SessionStatus::WaitingApproval || session.open_approvals.as_ref().is_none_or(|o| !o.is_empty())
}

#[test]
fn a_hub_set_to_allow_it_allows_what_an_agent_with_full_access_asks_and_nobody_is_told_it_waits() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.unwrap());
    client.hub_configure(allow_asked(true)).unwrap();
    let removed = to_remove(&worktree, "first");
    let feed = client.feed_subscribe().unwrap();

    client.session_prompt(&session.id, &format!("tool:{UNCHECKED_REMOVAL}"), None).unwrap();
    let told = told_until_the_turn_ends(&feed, &session.id);

    assert!(!removed.exists(), "it ran without anyone answering");
    let events = wait_for_turn_end(&client, &session.id, 1);
    let allowed = (Some((ApprovalOutcome::Allowed, true)), true);
    assert_eq!(answered(&events), [allowed], "the log says it was asked, and allowed by the setting");
    assert_eq!(told.iter().find(|session| waits(session)), None, "nobody is told it waits");
    let [chat, its_workspace, prompted] = last_active(&client, &session, &events);
    assert_eq!((chat, its_workspace), (prompted, prompted), "the person did nothing since they prompted it");

    // A subagent's request is the agent's own, so it's allowed the same.
    let removed = to_remove(&worktree, "second");
    client.session_prompt(&session.id, &format!("agents-ask:2:{UNCHECKED_REMOVAL}"), None).unwrap();
    let told = told_until_the_turn_ends(&feed, &session.id);

    assert!(!removed.exists());
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(answered(&events)[1..], [allowed, allowed]);
    assert_eq!(told.iter().find(|session| waits(session)), None);
}

#[test]
fn a_hub_not_set_to_allow_it_or_in_plan_mode_leaves_the_person_to_answer() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let removed = to_remove(&PathBuf::from(workspace.path.clone().unwrap()), "kept");

    client.session_prompt(&session.id, &format!("tool:{UNCHECKED_REMOVAL}"), None).unwrap();

    wait_for_approval(&client, &session.id);
    wait_for_status(&client, &session.id, SessionStatus::WaitingApproval);
    assert!(removed.exists(), "nothing runs before the person allows it");

    // Plan mode asks before any call that changes anything, and the person answers whatever the hub is set to do.
    client.hub_configure(allow_asked(true)).unwrap();
    let planning =
        client.session_create(&workspace.id, AgentKind::Claude, in_mode(PermissionMode::Plan), None).unwrap();
    client.session_prompt(&planning.id, "tool:echo planned > planned.txt", None).unwrap();

    wait_for_approval(&client, &planning.id);
    wait_for_status(&client, &planning.id, SessionStatus::WaitingApproval);
}
