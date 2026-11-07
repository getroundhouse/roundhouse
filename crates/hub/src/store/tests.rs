use protocol::event::{EventBody, NoticeLevel};
use protocol::model::{AgentKind, DiffComment, Repo, SessionStatus, SetupStatus, Workspace, WorkspaceStatus};

use super::*;

fn store_with_session(dir: &Path) -> Store {
    let store = Store::open(&dir.join("hub.db")).unwrap();
    let now = now_ms();
    store.add_repo(&Repo { id: "r".into(), name: "repo".into(), path: "/repo".into(), created_at_ms: now }).unwrap();
    store
        .insert_workspace(&Workspace {
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
        })
        .unwrap();
    store
        .insert_session(&StoredSession {
            session: Session {
                id: "s".into(),
                workspace_id: "w".into(),
                agent: AgentKind::Claude,
                model: None,
                permission_mode: None,
                title: None,
                closed_at_ms: None,
                status: SessionStatus::Idle,
                last_seq: 0,
                created_at_ms: now,
                updated_at_ms: now,
            },
            agent_session_id: "s".into(),
            agent_opened: false,
        })
        .unwrap();
    store
}

fn notice(text: &str) -> EventBody {
    EventBody::Notice { level: NoticeLevel::Info, message: text.into() }
}

#[test]
fn sequence_numbers_start_at_one_with_no_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for expected in 1..=5 {
        assert_eq!(store.append("s", &notice("x"), None, None, None).unwrap().event.seq, expected);
    }
    let events = store.read_events("s", 1, 100, u64::MAX, false).unwrap();
    assert_eq!(events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    assert_eq!(store.get_session("s").unwrap().session.last_seq, 5);
}

#[test]
fn a_status_change_commits_with_its_event() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.append("s", &notice("x"), None, None, Some(SessionStatus::Running)).unwrap();
    assert_eq!(store.get_session("s").unwrap().session.status, SessionStatus::Running);
    store.append("s", &notice("x"), None, None, None).unwrap();
    assert_eq!(store.get_session("s").unwrap().session.status, SessionStatus::Running);
}

#[test]
fn a_repeated_client_msg_id_returns_the_original_and_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = store.append("s", &notice("a"), None, Some("msg-1"), None).unwrap();
    assert!(!first.duplicate);
    store.append("s", &notice("b"), None, None, None).unwrap();
    let again = store.append("s", &notice("c"), None, Some("msg-1"), None).unwrap();
    assert!(again.duplicate);
    assert_eq!(again.event.seq, first.event.seq);
    assert_eq!(again.event.body, notice("a"));
    assert_eq!(store.get_session("s").unwrap().session.last_seq, 2);
}

#[test]
fn reads_page_by_count_and_by_size_and_always_make_progress() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for _ in 0..10 {
        store.append("s", &notice(&"x".repeat(1000)), None, None, None).unwrap();
    }
    assert_eq!(store.read_events("s", 1, 3, u64::MAX, false).unwrap().len(), 3);
    assert_eq!(store.read_events("s", 9, 100, u64::MAX, false).unwrap().len(), 2);
    assert_eq!(store.read_events("s", 11, 100, u64::MAX, false).unwrap().len(), 0);
    // Stops at the size limit, but a single event larger than the limit still comes back.
    assert_eq!(store.read_events("s", 1, 100, 2500, false).unwrap().len(), 3);
    assert_eq!(store.read_events("s", 1, 100, 1, false).unwrap().len(), 1);
}

#[test]
fn raw_lines_are_kept_and_only_returned_when_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.append("s", &notice("x"), Some("{\"original\":true}"), None, None).unwrap();
    assert_eq!(store.read_events("s", 1, 1, u64::MAX, false).unwrap()[0].raw, None);
    assert_eq!(store.read_events("s", 1, 1, u64::MAX, true).unwrap()[0].raw.as_deref(), Some("{\"original\":true}"));
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
    store.append("s", &requested("a"), None, None, None).unwrap();
    store.append("s", &requested("b"), None, None, None).unwrap();
    store.append("s", &cancelled_approval("a".into()), None, None, None).unwrap();
    assert_eq!(store.open_approvals("s").unwrap(), vec!["b".to_string()]);
}

#[test]
fn data_survives_reopening_and_migrations_do_not_rerun() {
    let dir = tempfile::tempdir().unwrap();
    store_with_session(dir.path()).append("s", &notice("kept"), None, None, None).unwrap();
    let store = Store::open(&dir.path().join("hub.db")).unwrap();
    assert_eq!(store.read_events("s", 1, 10, u64::MAX, false).unwrap()[0].body, notice("kept"));
}

#[test]
fn a_database_from_a_newer_version_is_refused_rather_than_damaged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    drop(Store::open(&path).unwrap());
    Connection::open(&path).unwrap().pragma_update(None, "user_version", 999).unwrap();
    assert!(Store::open(&path).is_err());
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
    };
    store.add_comment(&comment).unwrap();
    assert_eq!(store.list_comments("w").unwrap(), vec![comment]);
    assert_eq!(store.list_comments("other").unwrap(), vec![]);
    store.delete_comment("c1").unwrap();
    assert_eq!(store.list_comments("w").unwrap(), vec![]);
}

#[test]
fn each_workspace_gets_its_own_ports() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(store.next_port_base().unwrap(), 55_010);
    let mut next = store.get_workspace("w").unwrap();
    next.id = "w2".into();
    next.name = "birch".into();
    next.port_base = 55_010;
    store.insert_workspace(&next).unwrap();
    assert_eq!(store.next_port_base().unwrap(), 55_020);
}

#[test]
fn a_setup_cut_off_by_a_restart_counts_as_failed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_workspace_setup("w", SetupStatus::Running).unwrap();
    store.fail_interrupted_setups().unwrap();
    assert_eq!(store.get_workspace("w").unwrap().setup, SetupStatus::Failed { code: None });
    store.set_workspace_setup("w", SetupStatus::Succeeded).unwrap();
    store.fail_interrupted_setups().unwrap();
    assert_eq!(store.get_workspace("w").unwrap().setup, SetupStatus::Succeeded);
}

#[test]
fn adding_a_repo_twice_returns_the_same_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let again = store
        .add_repo(&Repo { id: "other".into(), name: "repo".into(), path: "/repo".into(), created_at_ms: 0 })
        .unwrap();
    assert_eq!(again.id, "r");
}

#[test]
fn workspace_names_are_unique_per_repo() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut duplicate = store.get_workspace("w").unwrap();
    duplicate.id = "w2".into();
    assert!(!store.insert_workspace(&duplicate).unwrap());
}
