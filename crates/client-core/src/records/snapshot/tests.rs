use protocol::client::HubNotice;
use protocol::model::{DiffComment, DiffSide, RecordKey, Records, Repo, Rev, Session, Subagent, Title, Workspace};

use crate::records::HubRecords;

fn repo(id: &str, name: &str) -> Repo {
    Repo { id: id.into(), name: name.into(), path: format!("/{name}"), created_at_ms: 0, rev: Rev::new(1) }
}

fn workspace(id: &str, created_at_ms: i64, rev: u64) -> Workspace {
    Workspace { created_at_ms, rev: Rev::new(rev), ..protocol::fixtures::workspace(id) }
}

fn session(id: &str, workspace: &str, created_at_ms: i64, rev: u64) -> Session {
    Session {
        workspace_id: workspace.into(),
        created_at_ms,
        title: Title::tidied(id),
        rev: Rev::new(rev),
        ..protocol::fixtures::session(id)
    }
}

fn comment(id: &str, workspace: &str, rev: u64) -> DiffComment {
    DiffComment {
        id: id.into(),
        workspace_id: workspace.into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "why".into(),
        created_at_ms: 0,
        rev: Rev::new(rev),
    }
}

fn subagent(session: &str, call: &str, started_at_ms: i64, rev: u64) -> Subagent {
    Subagent { session_id: session.into(), started_at_ms, rev: Rev::new(rev), ..protocol::fixtures::subagent(call) }
}

/// Two workspaces, each with a session, and a comment on the first's diff.
fn listed() -> Records {
    Records {
        repos: vec![repo("r2", "zeta"), repo("r", "alpha")],
        workspaces: vec![workspace("w2", 20, 3), workspace("w", 10, 2)],
        sessions: vec![session("s2", "w", 30, 5), session("s", "w", 10, 4), session("t", "w2", 0, 6)],
        comments: vec![comment("c", "w", 7)],
        rev: Rev::new(7),
        ..Records::default()
    }
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> &str) -> Vec<&str> {
    items.iter().map(id).collect()
}

#[test]
fn each_kind_is_listed_in_the_order_views_show_it_and_found_by_its_id() {
    let mut held = HubRecords::default();
    held.apply_list(listed());

    let shown = held.snapshot();

    assert_eq!(ids(shown.repos(), |r| r.name.as_str()), ["alpha", "zeta"], "by name");
    assert_eq!(ids(shown.workspaces(), |w| w.id.as_str()), ["w", "w2"], "oldest first");
    assert_eq!(ids(shown.sessions(&"w".into()), |s| s.id.as_str()), ["s", "s2"], "oldest first");
    assert_eq!(shown.session(&"t".into()).map(|s| s.workspace_id.as_str()), Some("w2"));
    assert_eq!(shown.workspace(&"w2".into()).map(|w| w.created_at_ms), Some(20));
    assert_eq!(shown.repo(&"r".into()).map(|r| r.name.as_str()), Some("alpha"));
    assert_eq!(ids(shown.comments(&"w".into()), |c| c.id.as_str()), ["c"]);
    assert!(shown.comments(&"w2".into()).is_empty() && shown.sessions(&"gone".into()).is_empty());
    assert_eq!(shown.every_session().count(), 3);
}

#[test]
fn a_snapshot_stays_as_it_was_taken_and_is_made_once_for_each_change() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    assert!(!held.snapshot().changes_since(&before).any(), "nothing changed, so it's the same one");
    held.apply_notice(HubNotice::Session(session("s", "w", 10, 8)));

    assert_eq!(before.session(&"s".into()).map(|s| s.rev), Some(Rev::new(4)));
    assert_eq!(held.snapshot().session(&"s".into()).map(|s| s.rev), Some(Rev::new(8)));
}

#[test]
fn a_view_is_told_which_parts_changed_since_it_last_looked() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    held.apply_notice(HubNotice::Session(session("s2", "w", 30, 8)));
    held.apply_notice(HubNotice::Removed { record: RecordKey::Comment { id: "c".into() }, rev: Rev::new(9) });
    let now = held.snapshot();
    let changes = now.changes_since(&before);

    assert!(changes.any());
    assert!(changes.sessions(&"w".into()) && !changes.sessions(&"w2".into()), "only the workspace it's in");
    assert!(changes.comments(&"w".into()), "one went");
    assert!(!changes.workspace(&"w".into()) && !changes.workspace(&"w2".into()), "no workspace's own record");
    assert!(!now.changes_since(&now).any());
}

#[test]
fn a_record_that_goes_is_a_change_even_when_it_was_not_the_newest() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    held.apply_notice(HubNotice::Session(session("s", "w", 10, 8)));
    let before = held.snapshot();

    held.apply_notice(HubNotice::Removed { record: RecordKey::Session { id: "s2".into() }, rev: Rev::new(9) });
    let now = held.snapshot();

    assert!(now.changes_since(&before).sessions(&"w".into()));
    assert_eq!(ids(now.sessions(&"w".into()), |s| s.id.as_str()), ["s"]);
}

#[test]
fn the_same_copy_listed_again_is_no_change_to_a_view() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    let mut again = listed();
    again.rev = Rev::new(8);
    held.apply_list(again);
    let changes_now = held.snapshot();
    let changes = changes_now.changes_since(&before);

    assert!(!changes.sessions(&"w".into()) && !changes.workspace(&"w".into()) && !changes.comments(&"w".into()));
}

/// From a hub that keeps no revisions, every copy has the same, so only when each was taken in tells them apart.
#[test]
fn a_change_from_a_hub_without_revisions_is_still_told_of() {
    let mut held = HubRecords::default();
    held.apply_list(Records { workspaces: vec![workspace("w", 10, 0)], ..Records::default() });
    let before = held.snapshot();

    let mut renamed = workspace("w", 10, 0);
    renamed.title = Title::tidied("Renamed");
    held.apply_notice(HubNotice::Workspace(renamed));
    let now = held.snapshot();

    assert!(now.changes_since(&before).workspace(&"w".into()));
}

#[test]
fn a_sessions_subagents_are_listed_as_they_started_and_found_by_their_call() {
    let mut held = HubRecords::default();
    held.subagents_mut().want("s".into());
    held.subagents_mut().apply_list(&"s".into(), vec![subagent("s", "b", 2, 5), subagent("s", "a", 1, 4)]);
    let before = held.snapshot();

    held.apply_notice(HubNotice::Subagent(subagent("s", "a", 1, 6)));
    let now = held.snapshot();

    assert_eq!(ids(now.subagents(&"s".into()), |s| s.call.as_str()), ["a", "b"]);
    assert_eq!(now.subagent(&"s".into(), &"a".into()).map(|s| s.rev), Some(Rev::new(6)));
    assert!(now.changes_since(&before).subagents(&"s".into()));
    assert!(!now.changes_since(&before).subagents(&"t".into()));
    let unchanged = |snapshot: &super::Snapshot| snapshot.subagent(&"s".into(), &"b".into()).cloned().unwrap();
    assert!(std::sync::Arc::ptr_eq(&unchanged(&before), &unchanged(&now)), "one that didn't change is shared");
}
