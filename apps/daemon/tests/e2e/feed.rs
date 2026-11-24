//! The feed: a notice for each record that changes, and a client's copy of the records kept from it.

#![cfg(test)]

use client_core::records::{HubRecords, Snapshot};
use client_core::{RecordsFeed, Subscription};
use protocol::client::HubNotice;
use protocol::model::{
    AgentKind, DiffSide, PullRequestLinked, RecordKey, Rev, Seq, Session, SessionStatus, WorkspaceStatus,
};

use crate::support::{Stack, next_notice_of, session_of, wait_for_setup, wait_for_turn_end, wait_until, workspace_of};

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
    Session { rev: Rev::default(), last_seq: Seq::default(), updated_at_ms: 0, ..session.clone() }
}

/// Whether `notice` is of what a sidebar shows: a repository, a workspace or a chat, or one's going. Not of an account,
/// which a chat runs under, or of the GitHub login found on the local provider's machine, which a sidebar doesn't
/// show, and which come anywhere among the others, whenever the agent says which account it runs under and the hub's
/// first worker says who `gh` is logged in as.
fn shown(notice: &HubNotice) -> bool {
    match notice {
        HubNotice::Repo(_) | HubNotice::Workspace(_) | HubNotice::Session(_) => true,
        HubNotice::Removed { record, .. } => {
            matches!(record, RecordKey::Repo { .. } | RecordKey::Workspace { .. } | RecordKey::Session { .. })
        }
        _ => false,
    }
}

impl Sidebar {
    /// The next notice that changes what a sidebar shows. A chat's record also says how full its agent's context is,
    /// which the agent says whenever it gets to it, and which account it runs under, so a notice of only that comes
    /// anywhere among the others. Each notice of a chat has to be a newer copy than the last and say something the
    /// last didn't, so nothing is announced twice, or for an event that changed nothing.
    fn next_notice(&mut self, feed: &Subscription, what: &str) -> HubNotice {
        loop {
            let notice = next_notice_of(feed, what, shown);
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
    let feed = client.follow(protocol::client::subscriptions::Follow::Hub).unwrap();

    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    assert_eq!(next_notice_of(&feed, "the repo", shown), HubNotice::Repo(repo.clone()));
    let workspace = client.workspace_create(&repo.id, Some("watched".into()), None, None).unwrap();
    // On its way as soon as it's recorded, then ready, then its setup running and finishing.
    let making = workspace_of(next_notice_of(&feed, "the workspace on its way", shown));
    assert_eq!((making.id.as_str(), making.status), (workspace.id.as_str(), WorkspaceStatus::Creating));
    let ready = workspace_of(next_notice_of(&feed, "the workspace", shown));
    assert_eq!((ready.id.as_str(), ready.status), (workspace.id.as_str(), WorkspaceStatus::Ready));
    assert!(ready.rev > making.rev);
    wait_for_setup(&client, &workspace.id);
    for _ in 0..2 {
        assert!(matches!(next_notice_of(&feed, "setup", shown), HubNotice::Workspace(_)));
    }
    let session = client
        .session_create(&workspace.id, AgentKind::Claude, protocol::capability::settings::Settings::default(), None)
        .unwrap();
    assert_eq!(next_notice_of(&feed, "the session", shown), HubNotice::Session(session.clone()));
    // Making a chat is doing something in its workspace, which moves up the sidebar.
    let made_a_chat = workspace_of(next_notice_of(&feed, "a chat made in it", shown));
    assert_eq!(made_a_chat.user_active_at_ms, Some(session.created_at_ms));

    // One notice per change of status, not one per event. Once the first prompt is recorded, it titles the
    // workspace and the chat.
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    let mut sidebar = Sidebar::default();
    let running = session_of(sidebar.next_notice(&feed, "running"));
    assert_eq!((running.id.as_str(), running.status), (session.id.as_str(), SessionStatus::Running));
    // And so is sending its agent something.
    let sent = workspace_of(sidebar.next_notice(&feed, "a prompt sent in it"));
    assert_eq!(sent.user_active_at_ms, running.user_active_at_ms);
    let titled = workspace_of(sidebar.next_notice(&feed, "the workspace's title"));
    assert_eq!((titled.id.as_str(), titled.title.as_deref()), (workspace.id.as_str(), Some("say:hello")));
    let titled = session_of(sidebar.next_notice(&feed, "the chat's title"));
    assert_eq!((titled.status, titled.title.as_deref()), (SessionStatus::Running, Some("say:hello")));
    let idle = session_of(sidebar.next_notice(&feed, "idle"));
    assert_eq!(idle.status, SessionStatus::Idle);
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
    // The turn's end reads its pull request, which a repository with no GitHub remote doesn't have.
    let looked = workspace_of(sidebar.next_notice(&feed, "its pull request"));
    let none = Some(PullRequestLinked { link: None });
    assert_eq!((looked.id.as_str(), looked.pull_request_link), (workspace.id.as_str(), none));

    // Each is announced as it starts, so every client shows it on its way out or back, and again once it's done.
    let archived = client.workspace_archive(&workspace.id, true).unwrap();
    let going = workspace_of(sidebar.next_notice(&feed, "archiving"));
    assert_eq!((going.id.as_str(), going.status), (archived.id.as_str(), WorkspaceStatus::Archiving));
    assert_eq!(sidebar.next_notice(&feed, "the archive"), HubNotice::Workspace(archived));
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let coming = workspace_of(sidebar.next_notice(&feed, "restoring"));
    assert_eq!((coming.id.as_str(), coming.status), (restored.id.as_str(), WorkspaceStatus::Restoring));
    let seen = workspace_of(sidebar.next_notice(&feed, "the restore"));
    assert_eq!((seen.id.as_str(), seen.status), (restored.id.as_str(), WorkspaceStatus::Ready));
}

#[test]
fn a_client_copy_of_the_records_keeps_up_with_the_hub() {
    let stack = Stack::start();
    let client = stack.client();
    // On a connection of its own, so only the feed, and not the replies it would take in, can bring the changes.
    let feed = RecordsFeed::start(&stack.client());

    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let kept = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Kept", None).unwrap();
    let removed = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 2, "Removed", None).unwrap();
    client.comment_delete(&removed.id).unwrap();

    // Everything the feed announces. A session's newest event moves without a notice, so it's left out, and so is each
    // record's revision, which moves too as an observation whose old form the record holds changes, and a client of
    // this version is sent no copy that moves only that.
    let shown = |records: &Snapshot| {
        let mut workspaces = records.workspaces().to_vec();
        let mut sessions: Vec<_> = workspaces.iter().map(|w| records.sessions(&w.id).to_vec()).collect();
        for session in sessions.iter_mut().flatten() {
            (session.last_seq, session.updated_at_ms, session.rev) = (Seq::default(), 0, Rev::default());
        }
        for workspace in &mut workspaces {
            workspace.rev = Rev::default();
        }
        let comments: Vec<_> = workspaces.iter().map(|w| records.comments(&w.id).to_vec()).collect();
        (records.repos().to_vec(), workspaces, sessions, comments)
    };
    wait_until("the copy to match a list", || {
        let mut listed = HubRecords::default();
        listed.apply_list(client.records().unwrap());
        shown(&feed.snapshot()) == shown(&listed.snapshot())
    });
    assert_eq!(feed.snapshot().comments(&workspace.id), [kept]);
}

/// What the hub's agents offer reaches a client's copy from the adapters the daemon was built with.
#[test]
fn a_client_copy_holds_what_each_kind_of_agent_offers_as_its_adapter_describes_it() {
    let stack = Stack::start();
    let client = stack.client();

    let feed = RecordsFeed::start(&client);

    wait_until("the copy to have caught up", || feed.connected());
    assert_eq!(feed.snapshot().agents().of(AgentKind::Claude), Some(&agents::claude::description()));
    assert_eq!(client.agent_list().unwrap(), agents::descriptions());
}
