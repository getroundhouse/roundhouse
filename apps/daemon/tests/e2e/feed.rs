//! The feed: a notice for each record that changes, and a client's copy of the records kept from it.

#![cfg(test)]

use client_core::records::{HubRecords, Snapshot};
use client_core::{RecordsFeed, Subscription};
use protocol::client::HubNotice;
use protocol::model::{
    AgentKind, BranchName, DiffSide, Head, PullRequestFound, RecordKey, Rev, Session, SessionSettingsChange,
    SessionStatus, WorkspaceStatus,
};

use crate::support::{Stack, next_notice, wait_for_setup, wait_for_turn_end, wait_until};

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
    // On its way as soon as it's recorded, then ready, then its setup running and finishing.
    let HubNotice::Workspace(making) = next_notice(&feed, "the workspace on its way") else { panic!() };
    assert_eq!((making.id.as_str(), making.status), (workspace.id.as_str(), WorkspaceStatus::Creating));
    let HubNotice::Workspace(ready) = next_notice(&feed, "the workspace") else { panic!() };
    assert_eq!((ready.id.as_str(), ready.status), (workspace.id.as_str(), WorkspaceStatus::Ready));
    assert!(ready.rev > making.rev);
    // With the branch it's on, so nothing has to follow to say so.
    let on = ready.branches.map(|branches| branches.head);
    assert_eq!(on, Some(Head::Branch { name: BranchName::from(workspace.branch.as_str()) }));
    wait_for_setup(&client, &workspace.id);
    for _ in 0..2 {
        assert!(matches!(next_notice(&feed, "setup"), HubNotice::Workspace(_)));
    }
    let session =
        client.session_create(&workspace.id, AgentKind::Claude, SessionSettingsChange::default(), None).unwrap();
    assert_eq!(next_notice(&feed, "the session"), HubNotice::Session(session.clone()));
    // Making a chat is doing something in its workspace, which moves up the sidebar.
    let HubNotice::Workspace(made_a_chat) = next_notice(&feed, "a chat made in it") else { panic!() };
    assert_eq!(made_a_chat.user_active_at_ms, Some(session.created_at_ms));

    // One notice per change of status, not one per event. Once the first prompt is recorded, it titles the
    // workspace and the chat.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    let mut sidebar = Sidebar::default();
    let HubNotice::Session(running) = sidebar.next_notice(&feed, "running") else { panic!() };
    assert_eq!((running.id.as_str(), running.status), (session.id.as_str(), SessionStatus::Running));
    // And so is sending its agent something.
    let HubNotice::Workspace(sent) = sidebar.next_notice(&feed, "a prompt sent in it") else { panic!() };
    assert_eq!(sent.user_active_at_ms, running.user_active_at_ms);
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

/// So a client shows it on its way until it goes.
#[test]
fn a_workspace_that_cant_be_made_is_announced_on_its_way_and_then_removed() {
    let stack = Stack::start();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    next_notice(&feed, "the repo");
    std::fs::remove_dir_all(stack.repo()).unwrap();

    client.workspace_create(&repo.id, Some("doomed".into()), None, None).unwrap_err();

    let HubNotice::Workspace(making) = next_notice(&feed, "the workspace on its way") else { panic!() };
    assert_eq!((making.name.as_str(), making.status), ("doomed", WorkspaceStatus::Creating));
    let HubNotice::Removed { record, rev } = next_notice(&feed, "its removal") else { panic!() };
    assert_eq!(record, RecordKey::Workspace { id: making.id });
    assert!(rev > making.rev);
    assert!(client.workspace_list(true).unwrap().is_empty());
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
    let shown = |records: &Snapshot| {
        let workspaces = records.workspaces().to_vec();
        let mut sessions: Vec<_> = workspaces.iter().map(|w| records.sessions(&w.id).to_vec()).collect();
        for session in sessions.iter_mut().flatten() {
            (session.last_seq, session.updated_at_ms) = (0, 0);
        }
        let comments: Vec<_> = workspaces.iter().map(|w| records.comments(&w.id).to_vec()).collect();
        (records.repos().to_vec(), workspaces, sessions, comments)
    };
    wait_until("the copy to match a list", || {
        let mut listed = HubRecords::default();
        listed.apply_list(client.records_list().unwrap());
        shown(&feed.snapshot()) == shown(&listed.snapshot())
    });
    assert_eq!(feed.snapshot().comments(&workspace.id), [kept]);
}
