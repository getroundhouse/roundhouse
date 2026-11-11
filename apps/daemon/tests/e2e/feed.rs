//! The feed: a notice for each record that changes, and a client's copy of the records kept from it.

#![cfg(test)]

use std::time::Instant;

use client_core::records::HubRecords;
use client_core::{RecordsFeed, Subscription, SubscriptionItem};
use protocol::client::HubNotice;
use protocol::model::{AgentKind, DiffSide, PullRequestFound, RecordKey, Rev, Session, SessionStatus, WorkspaceStatus};

use crate::support::{Stack, TIMEOUT, in_mode, wait_for_setup, wait_for_turn_end, wait_until};

fn next_notice(feed: &Subscription, what: &str) -> HubNotice {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match feed.items.recv_timeout(wait) {
            Ok(SubscriptionItem::Notice(notice)) => return notice,
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for {what}: {e}"),
        }
    }
}

/// What a sidebar shows of the chat the feed last told of, its status and title, and the copy of its record that
/// said so.
#[derive(Default)]
struct Sidebar {
    chat: Option<(SessionStatus, Option<String>)>,
    told: Option<Session>,
}

/// A record without what every event moves, so two copies are the same if nothing about the chat changed between
/// them.
fn apart_from_the_log(session: &Session) -> Session {
    Session { rev: Rev::default(), last_seq: 0, updated_at_ms: 0, ..session.clone() }
}

impl Sidebar {
    /// The next notice that changes what a sidebar shows. A chat's record also says how full its agent's context is,
    /// which the agent says whenever it gets to it, and which account it runs under, so a notice of only that comes
    /// anywhere among the others, and so does one of the account's own record, which a sidebar doesn't show. Each
    /// notice of a chat has to be a newer copy than the last and say something the last didn't, so nothing is
    /// announced twice, or for an event that changed nothing.
    fn next_notice(&mut self, feed: &Subscription, what: &str) -> HubNotice {
        loop {
            let notice = next_notice(feed, what);
            if matches!(notice, HubNotice::Account(_)) {
                continue;
            }
            if let HubNotice::Session(session) = &notice {
                if let Some(last) = &self.told {
                    assert!(session.rev > last.rev, "a copy no newer than the last: {session:?}");
                    assert_ne!(apart_from_the_log(session), apart_from_the_log(last), "told what the last said");
                }
                self.told = Some(session.clone());
                let shown = Some((session.status, session.title.as_deref().map(str::to_string)));
                if shown == self.chat {
                    continue;
                }
                self.chat = shown;
            }
            return notice;
        }
    }
}

#[test]
fn the_feed_reports_what_a_sidebar_shows() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();

    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    assert_eq!(next_notice(&feed, "the repo"), HubNotice::Repo(repo.clone()));
    let workspace = client.workspace_create(&repo.id, Some("watched".into()), None, None).unwrap();
    // Ready, then its setup running and finishing.
    let HubNotice::Workspace(ready) = next_notice(&feed, "the workspace") else { panic!() };
    assert_eq!(ready.id, workspace.id);
    wait_for_setup(&client, &workspace.id);
    for _ in 0..2 {
        assert!(matches!(next_notice(&feed, "setup"), HubNotice::Workspace(_)));
    }
    let session = client.session_create(&workspace.id, AgentKind::Claude, in_mode("bypassPermissions"), None).unwrap();
    assert_eq!(next_notice(&feed, "the session"), HubNotice::Session(session.clone()));

    // One notice per change of status, not one per event. Once the first prompt is recorded, it titles the
    // workspace and the chat.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    let mut sidebar = Sidebar::default();
    let HubNotice::Session(running) = sidebar.next_notice(&feed, "running") else { panic!() };
    assert_eq!((running.id.as_str(), running.status), (session.id.as_str(), SessionStatus::Running));
    let HubNotice::Workspace(titled) = sidebar.next_notice(&feed, "the workspace's title") else { panic!() };
    assert_eq!((titled.id.as_str(), titled.title.as_deref()), (workspace.id.as_str(), Some("say:hello")));
    let HubNotice::Session(titled) = sidebar.next_notice(&feed, "the chat's title") else { panic!() };
    assert_eq!((titled.status, titled.title.as_deref()), (SessionStatus::Running, Some("say:hello")));
    let HubNotice::Session(idle) = sidebar.next_notice(&feed, "idle") else { panic!() };
    assert_eq!(idle.status, SessionStatus::Idle);
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
    // The turn's end reads its pull request, which a repository with no GitHub remote doesn't have.
    let HubNotice::Workspace(looked) = sidebar.next_notice(&feed, "its pull request") else { panic!() };
    let none = Some(PullRequestFound { pull_request: None });
    assert_eq!((looked.id.as_str(), looked.pull_request), (workspace.id.as_str(), none));

    let archived = client.workspace_archive(&workspace.id, true).unwrap();
    assert_eq!(sidebar.next_notice(&feed, "the archive"), HubNotice::Workspace(archived));
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let HubNotice::Workspace(seen) = sidebar.next_notice(&feed, "the restore") else { panic!() };
    assert_eq!((seen.id.as_str(), seen.status), (restored.id.as_str(), WorkspaceStatus::Ready));
}

#[test]
fn a_list_and_the_feed_agree_on_which_copy_is_newer() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();
    let (workspace, session) = stack.workspace(&client);
    let before = client.records_list().unwrap();

    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let comment = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this", None).unwrap();
    client.comment_delete(&comment.id).unwrap();
    let mut notices = Vec::new();
    while !matches!(notices.last(), Some(HubNotice::Removed { .. })) {
        notices.push(next_notice(&feed, "the comment's removal"));
    }
    let after = client.records_list().unwrap();

    // Each change since the first list is newer than it, and the second list is at least as new.
    let newest_session = notices.iter().rev().find_map(|n| match n {
        HubNotice::Session(s) if s.id == session.id && s.rev > before.rev => Some(s),
        _ => None,
    });
    let listed = after.sessions.iter().find(|s| s.id == session.id).unwrap();
    assert!(listed.rev >= newest_session.unwrap().rev);
    assert_eq!(listed.status, SessionStatus::Idle);
    let added = notices.iter().find_map(|n| match n {
        HubNotice::Comment(c) if c.id == comment.id => Some(c),
        _ => None,
    });
    assert_eq!(added, Some(&comment));
    let Some(HubNotice::Removed { record, rev }) = notices.last() else { unreachable!() };
    assert_eq!(record, &RecordKey::Comment { id: comment.id.clone() });
    assert!(before.rev < comment.rev && comment.rev < *rev && *rev <= after.rev);
    assert!(after.comments.is_empty());
}

#[test]
fn a_client_copy_of_the_records_keeps_up_with_the_hub() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = RecordsFeed::start(&client);

    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let kept = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Kept", None).unwrap();
    let removed = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 2, "Removed", None).unwrap();
    client.comment_delete(&removed.id).unwrap();

    // Everything the feed announces. A session's newest event moves without a notice, so it's left out.
    let shown = |records: &HubRecords| {
        let mut sessions = records.sessions();
        for session in sessions.values_mut().flatten() {
            (session.last_seq, session.updated_at_ms) = (0, 0);
        }
        (records.repos(), records.workspaces(), sessions, records.comments())
    };
    wait_until("the copy to match a list", || {
        let mut listed = HubRecords::default();
        listed.apply_list(client.records_list().unwrap());
        feed.read(shown) == shown(&listed)
    });
    assert_eq!(feed.read(HubRecords::comments).get(&workspace.id), Some(&vec![kept]));
}
