use protocol::event::{EventBody, NoticeLevel};
use protocol::model::{
    AgentKind, ApprovalId, ClientRequestId, DiffComment, Repo, Rev, SessionStatus, SetupStatus, Workspace,
    WorkspaceStatus,
};

use super::*;

/// The store in `dir`, with its backups beside it.
fn open_in(dir: &Path) -> Store {
    Store::open(&dir.join("hub.db"), &dir.join("backups")).unwrap()
}

/// A repository with one workspace, `w`, and one idle session in it, `s`.
pub(super) fn store_with_session(dir: &Path) -> Store {
    let store = open_in(dir);
    let now = now_ms();
    let repo =
        Repo { id: "r".into(), name: "repo".into(), path: "/repo".into(), created_at_ms: now, rev: Rev::default() };
    store.add_repo(&repo).unwrap();
    store
        .insert_workspace(
            &Workspace {
                id: "w".into(),
                repo_id: "r".into(),
                name: "alder".into(),
                branch: "alder".into(),
                base_ref: None,
                path: None,
                status: WorkspaceStatus::Ready,
                provider: "local".into(),
                instance_id: "w".into(),
                created_at_ms: now,
                archived_at_ms: None,
                setup: SetupStatus::NotStarted,
                port_base: 55_000,
                rev: Rev::default(),
            },
            None,
        )
        .unwrap();
    store
        .insert_session(
            &StoredSession {
                session: Session {
                    id: "s".into(),
                    workspace_id: "w".into(),
                    agent: AgentKind::Claude,
                    model: None,
                    permission_mode: None,
                    title: None,
                    closed_at_ms: None,
                    reply_seq: 0,
                    read_seq: 0,
                    background_tasks: Vec::new(),
                    rev: Rev::default(),
                    status: SessionStatus::Idle,
                    last_seq: 0,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
                agent_session_id: "s".into(),
                agent_opened: false,
            },
            None,
        )
        .unwrap();
    store
}

pub(super) fn notice(text: &str) -> EventBody {
    EventBody::Notice { level: NoticeLevel::Info, message: text.into() }
}

#[test]
fn sequence_numbers_start_at_one_with_no_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for expected in 1..=5 {
        assert_eq!(store.append(&"s".into(), None, &notice("x"), None, None, None).unwrap().event.seq, expected);
    }
    let events = store.read_events(&"s".into(), 1, 100, u64::MAX, false).unwrap();
    assert_eq!(events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    assert_eq!(store.get_session(&"s".into()).unwrap().session.last_seq, 5);
}

#[test]
fn a_status_change_commits_with_its_event() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.append(&"s".into(), None, &notice("x"), None, None, Some(SessionStatus::Running.into())).unwrap();
    assert_eq!(store.get_session(&"s".into()).unwrap().session.status, SessionStatus::Running);
    store.append(&"s".into(), None, &notice("x"), None, None, None).unwrap();
    assert_eq!(store.get_session(&"s".into()).unwrap().session.status, SessionStatus::Running);
}

#[test]
fn a_repeated_client_msg_id_returns_the_original_and_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = store.append(&"s".into(), None, &notice("a"), None, Some(&"msg-1".into()), None).unwrap();
    assert!(!first.duplicate);
    store.append(&"s".into(), None, &notice("b"), None, None, None).unwrap();
    let again = store.append(&"s".into(), None, &notice("c"), None, Some(&"msg-1".into()), None).unwrap();
    assert!(again.duplicate);
    assert_eq!(again.event.seq, first.event.seq);
    assert_eq!(again.event.body, notice("a"));
    assert_eq!(store.get_session(&"s".into()).unwrap().session.last_seq, 2);
}

#[test]
fn reads_page_by_count_and_by_size_and_always_make_progress() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for _ in 0..10 {
        store.append(&"s".into(), None, &notice(&"x".repeat(1000)), None, None, None).unwrap();
    }
    assert_eq!(store.read_events(&"s".into(), 1, 3, u64::MAX, false).unwrap().len(), 3);
    assert_eq!(store.read_events(&"s".into(), 9, 100, u64::MAX, false).unwrap().len(), 2);
    assert_eq!(store.read_events(&"s".into(), 11, 100, u64::MAX, false).unwrap().len(), 0);
    // Stops at the size limit, but a single event larger than the limit still comes back.
    assert_eq!(store.read_events(&"s".into(), 1, 100, 2500, false).unwrap().len(), 3);
    assert_eq!(store.read_events(&"s".into(), 1, 100, 1, false).unwrap().len(), 1);
}

#[test]
fn raw_lines_are_kept_and_only_returned_when_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.append(&"s".into(), None, &notice("x"), Some("{\"original\":true}"), None, None).unwrap();
    assert_eq!(store.read_events(&"s".into(), 1, 1, u64::MAX, false).unwrap()[0].raw, None);
    assert_eq!(
        store.read_events(&"s".into(), 1, 1, u64::MAX, true).unwrap()[0].raw.as_deref(),
        Some("{\"original\":true}")
    );
}

#[test]
fn open_approvals_are_those_requested_and_not_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let requested = |id: &str| EventBody::ApprovalRequested {
        approval_id: id.into(),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: Value::Null,
        description: None,
        suggestions: Value::Null,
    };
    store.append(&"s".into(), None, &requested("a"), None, None, None).unwrap();
    store.append(&"s".into(), None, &requested("b"), None, None, None).unwrap();
    store.append(&"s".into(), None, &cancelled_approval("a".into()), None, None, None).unwrap();
    assert_eq!(store.open_approvals(&"s".into()).unwrap(), vec![ApprovalId::from("b")]);
}

#[test]
fn data_survives_reopening_and_migrations_do_not_rerun() {
    let dir = tempfile::tempdir().unwrap();
    store_with_session(dir.path()).append(&"s".into(), None, &notice("kept"), None, None, None).unwrap();
    let store = open_in(dir.path());
    assert_eq!(store.read_events(&"s".into(), 1, 10, u64::MAX, false).unwrap()[0].body, notice("kept"));
}

#[test]
fn an_event_keeps_its_run_and_no_run_is_handed_out_twice() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = store.next_run().unwrap();
    store.append(&"s".into(), Some(first), &notice("from the agent"), None, None, None).unwrap();
    store.append(&"s".into(), None, &notice("from the hub"), None, None, None).unwrap();
    drop(store);

    let store = open_in(dir.path());
    let second = store.next_run().unwrap();

    assert!(second > first, "a reopened store handed out {second} after {first}");
    let events = store.read_events(&"s".into(), 1, 10, u64::MAX, false).unwrap();
    assert_eq!(events.iter().map(|e| e.run).collect::<Vec<_>>(), vec![Some(first), None]);
}

#[test]
fn a_database_from_before_runs_and_revisions_keeps_everything_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let mut conn = schema::open_connection(&path).unwrap();
    schema::migrate_to(&mut conn, &backups, schema::BEFORE_RUNS).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO repos VALUES ('r', 'repo', '/repo', 0);
        INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms)
            VALUES ('w', 'r', 'alder', 'alder', 'ready', 'local', 'w', 0);
        INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms, updated_at_ms)
            VALUES ('s', 'w', 'claude', 's', 'idle', 1, 0, 0);
        INSERT INTO events (session_id, seq, ts_ms, kind, body, raw)
            VALUES ('s', 1, 0, 'notice', '{"type":"notice","level":"info","message":"old"}', 'line');
        "#,
    )
    .unwrap();
    drop(conn);

    let store = open_in(dir.path());

    let events = store.read_events(&"s".into(), 1, 10, u64::MAX, true).unwrap();
    assert_eq!((events[0].run, &events[0].body, events[0].raw.as_deref()), (None, &notice("old"), Some("line")));
    assert_eq!(store.append(&"s".into(), None, &notice("new"), None, None, None).unwrap().event.seq, 2);
    // Records from before revisions are at the first one, and anything after moves past it.
    let records = store.list_records().unwrap();
    assert_eq!(
        (records.repos[0].rev, records.workspaces[0].rev, records.sessions[0].rev),
        (Rev::new(1), Rev::new(1), Rev::new(1))
    );
    store.set_session_title(&"s".into(), "t").unwrap();
    assert!(store.get_session(&"s".into()).unwrap().session.rev > Rev::new(1));
}

/// Every way a record changes moves its revision past everything before it, and a list says how far
/// it reaches, so a client can always tell which of two copies is newer.
#[test]
fn every_change_to_a_record_gives_it_a_newer_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut seen = store.list_records().unwrap().rev;
    let mut newer = |rev: Rev, what: &str| {
        assert!(rev > seen, "{what} left revision {rev}, which isn't past {seen}");
        seen = rev;
    };
    let session = |store: &Store| store.get_session(&"s".into()).unwrap().session.rev;
    let workspace = |store: &Store| store.get_workspace(&"w".into()).unwrap().rev;

    store.append(&"s".into(), None, &notice("x"), None, None, Some(SessionStatus::Running.into())).unwrap();
    let status_changed = session(&store);
    newer(status_changed, "a change of status");
    store.append(&"s".into(), None, &notice("y"), None, None, None).unwrap();
    assert_eq!(session(&store), status_changed, "an event alone isn't announced, so it leaves the revision");
    store.set_session_options(&"s".into(), Some("model"), None).unwrap();
    newer(session(&store), "a model");
    store.set_session_title(&"s".into(), "title").unwrap();
    newer(session(&store), "a title");
    store.set_session_closed(&"s".into(), Some(1)).unwrap();
    newer(session(&store), "closing");
    newer(store.workspace_ready(&"w".into(), "/w", "alder", "main").unwrap().rev, "a ready workspace");
    newer(store.set_workspace_setup(&"w".into(), SetupStatus::Running).unwrap().rev, "setup");
    store.set_workspace_branch(&"w".into(), "renamed").unwrap();
    newer(workspace(&store), "a branch");
    newer(store.set_workspace_archived(&"w".into(), true).unwrap().rev, "archiving");
    let mut half_made = store.get_workspace(&"w".into()).unwrap();
    half_made.id = "w2".into();
    half_made.name = "birch".into();
    store.insert_workspace(&half_made, None).unwrap();
    newer(store.get_workspace(&"w2".into()).unwrap().rev, "a new workspace");
    newer(store.delete_workspace(&"w2".into()).unwrap().unwrap(), "removing a workspace");
    let mut another = store.get_session(&"s".into()).unwrap();
    another.session.id = "s2".into();
    store.insert_session(&another, None).unwrap();
    newer(store.get_session(&"s2".into()).unwrap().session.rev, "a new session");
    let comment = DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: protocol::model::DiffSide::New,
        line: 1,
        text: "x".into(),
        created_at_ms: 0,
        rev: Rev::default(),
    };
    let Made::New(added) = store.add_comment(&comment, None).unwrap() else { panic!("made before") };
    newer(added.rev, "a comment");
    newer(store.delete_comment(&"c".into()).unwrap().unwrap(), "removing a comment");
    let repo = Repo { id: "r2".into(), name: "two".into(), path: "/two".into(), created_at_ms: 0, rev: Rev::default() };
    newer(store.add_repo(&repo).unwrap().rev, "a repository");

    let records = store.list_records().unwrap();
    assert_eq!(records.rev, seen);
    let revs = records.repos.iter().map(|r| r.rev).chain(records.workspaces.iter().map(|w| w.rev));
    assert!(revs.chain(records.sessions.iter().map(|s| s.rev)).all(|rev| rev <= records.rev));
    assert_eq!(store.delete_comment(&"c".into()).unwrap(), None, "nothing to remove");
}

/// Two of the same request can both get past the hub's check before either is stored, so the store finds
/// what the first made, under the same lock as the insert.
#[test]
fn a_request_sent_again_gets_what_it_made_the_first_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let mut workspace = store.get_workspace(&"w".into()).unwrap();
    let request = ClientRequestId::from("make a workspace");
    (workspace.id, workspace.name) = ("w2".into(), "birch".into());
    let Some(Made::New(first)) = store.insert_workspace(&workspace, Some(&request)).unwrap() else { panic!() };
    (workspace.id, workspace.name) = ("w3".into(), "cedar".into());
    let again = store.insert_workspace(&workspace, Some(&request)).unwrap();
    assert_eq!(again, Some(Made::Earlier(first)), "a workspace");

    let mut session = store.get_session(&"s".into()).unwrap();
    let request = ClientRequestId::from("make a session");
    session.session.id = "s2".into();
    let Made::New(first) = store.insert_session(&session, Some(&request)).unwrap() else { panic!() };
    session.session.id = "s3".into();
    let Made::Earlier(again) = store.insert_session(&session, Some(&request)).unwrap() else { panic!("a session") };
    assert_eq!(again.session, first.session);

    let mut comment = DiffComment {
        id: "c1".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: protocol::model::DiffSide::New,
        line: 1,
        text: "x".into(),
        created_at_ms: 0,
        rev: Rev::default(),
    };
    let request = ClientRequestId::from("add a comment");
    let Made::New(first) = store.add_comment(&comment, Some(&request)).unwrap() else { panic!() };
    comment.id = "c2".into();
    assert_eq!(store.add_comment(&comment, Some(&request)).unwrap(), Made::Earlier(first), "a comment");
    assert_eq!(store.list_comments(&"w".into()).unwrap().len(), 1);
}

#[test]
fn comments_belong_to_a_workspace_and_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let comment = DiffComment {
        id: "c1".into(),
        workspace_id: "w".into(),
        path: "src/a.rs".into(),
        side: protocol::model::DiffSide::New,
        line: 12,
        text: "Rename this".into(),
        created_at_ms: 1,
        rev: Rev::default(),
    };
    let Made::New(stored) = store.add_comment(&comment, None).unwrap() else { panic!("made before") };
    assert_eq!(stored, DiffComment { rev: stored.rev, ..comment });
    assert_eq!(store.list_comments(&"w".into()).unwrap(), vec![stored]);
    assert_eq!(store.list_comments(&"other".into()).unwrap(), vec![]);
    store.delete_comment(&"c1".into()).unwrap();
    assert_eq!(store.list_comments(&"w".into()).unwrap(), vec![]);
}

#[test]
fn each_workspace_gets_its_own_ports() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(store.next_port_base().unwrap(), 55_010);
    let mut next = store.get_workspace(&"w".into()).unwrap();
    next.id = "w2".into();
    next.name = "birch".into();
    next.port_base = 55_010;
    store.insert_workspace(&next, None).unwrap();
    assert_eq!(store.next_port_base().unwrap(), 55_020);
}

#[test]
fn a_setup_cut_off_by_a_restart_counts_as_failed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_workspace_setup(&"w".into(), SetupStatus::Running).unwrap();
    store.fail_interrupted_setups().unwrap();
    assert_eq!(store.get_workspace(&"w".into()).unwrap().setup, SetupStatus::Failed { code: None });
    store.set_workspace_setup(&"w".into(), SetupStatus::Succeeded).unwrap();
    store.fail_interrupted_setups().unwrap();
    assert_eq!(store.get_workspace(&"w".into()).unwrap().setup, SetupStatus::Succeeded);
}

#[test]
fn adding_a_repo_twice_returns_the_same_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let again = store
        .add_repo(&Repo {
            id: "other".into(),
            name: "repo".into(),
            path: "/repo".into(),
            created_at_ms: 0,
            rev: Rev::default(),
        })
        .unwrap();
    assert_eq!(again.id, "r".into());
}

#[test]
fn workspace_names_are_unique_per_repo() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut duplicate = store.get_workspace(&"w".into()).unwrap();
    duplicate.id = "w2".into();
    assert_eq!(store.insert_workspace(&duplicate, None).unwrap(), None);
}
