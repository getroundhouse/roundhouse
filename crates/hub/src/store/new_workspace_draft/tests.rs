use protocol::attachment::MediaType;
use protocol::model::{Effort, ModelName, RepoId};

const OPUS: ModelName = ModelName::named("opus");

use super::*;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_NEW_WORKSPACE_DRAFT_NAMES;
use crate::store::tests::store_with_session;

fn log_file(id: &str) -> Attachment {
    Attachment { id: id.into(), name: format!("{id}.log"), media_type: MediaType::Text, size_bytes: 3 }
}

fn typed(text: &str, files: &[&str]) -> NewWorkspaceDraftToKeep {
    NewWorkspaceDraftToKeep {
        repo_id: Some(RepoId::from("r")),
        text: text.to_string(),
        files: files.iter().map(|&id| id.into()).collect(),
        model: Some(OPUS),
        effort: Some(Effort::High),
        plan: true,
    }
}

#[test]
fn a_hub_starts_with_nothing_kept_and_a_revision_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let draft = store.new_workspace_draft().unwrap();

    assert_eq!(draft, NewWorkspaceDraft { rev: draft.rev, ..NewWorkspaceDraft::default() });
    assert!(draft.rev > Rev::default(), "a record a client can tell is newer than none");
    assert_eq!(store.list_records().unwrap().new_workspace_draft, Some(draft));
}

#[test]
fn whats_kept_is_there_until_something_else_is_and_the_same_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.new_workspace_draft().unwrap().rev;

    let (kept, changed) = store.set_new_workspace_draft(&typed("fix the login", &[])).unwrap();
    let (again, changed_again) = store.set_new_workspace_draft(&typed("fix the login", &[])).unwrap();
    let (replaced, _) = store.set_new_workspace_draft(&typed("fix the logout", &[])).unwrap();

    assert!(changed && kept.rev > before);
    assert_eq!((kept.repo_id.as_ref(), kept.text.as_str()), (Some(&RepoId::from("r")), "fix the login"));
    assert_eq!((kept.model.clone(), kept.effort, kept.plan), (Some(OPUS), Some(Effort::High), true));
    assert!(!changed_again);
    assert_eq!(again, kept, "the same revision");
    assert!(replaced.rev > kept.rev);
    assert_eq!(store.new_workspace_draft().unwrap(), replaced);
}

#[test]
fn its_files_are_named_as_the_hub_has_them_and_one_it_hasnt_been_given_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_new_workspace_draft_file(&log_file("a1")).unwrap();

    let (kept, _) = store.set_new_workspace_draft(&typed("", &["a1", "a2"])).unwrap();

    assert_eq!(kept.files, [log_file("a1")]);
}

#[test]
fn adding_a_file_again_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_new_workspace_draft_file(&log_file("a1")).unwrap();

    let again = store.add_new_workspace_draft_file(&Attachment { name: "other.log".into(), ..log_file("a1") }).unwrap();

    assert_eq!(again, log_file("a1"));
}

#[test]
fn a_file_is_let_go_of_only_once_the_draft_no_longer_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_new_workspace_draft_file(&log_file("a1")).unwrap();
    store.set_new_workspace_draft(&typed("", &["a1"])).unwrap();

    let while_named = store.let_go_of_new_workspace_draft_file(&"a1".into()).unwrap();
    store.set_new_workspace_draft(&typed("", &[])).unwrap();
    let once_not = store.let_go_of_new_workspace_draft_file(&"a1".into()).unwrap();
    let again = store.let_go_of_new_workspace_draft_file(&"a1".into()).unwrap();

    assert!(!while_named);
    assert!(once_not);
    assert!(!again, "nothing more to let go of");
    assert_eq!(store.new_workspace_draft_file(&"a1".into()).unwrap(), None);
}

#[test]
fn a_file_nothing_names_is_let_go_of_once_its_waited_long_enough() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for id in ["named", "unnamed"] {
        store.add_new_workspace_draft_file(&log_file(id)).unwrap();
    }
    store.set_new_workspace_draft(&typed("", &["named"])).unwrap();

    let too_soon = store.let_go_of_unnamed_new_workspace_draft_files(now_ms() - 60_000).unwrap();
    let after = store.let_go_of_unnamed_new_workspace_draft_files(now_ms() + 1).unwrap();

    assert_eq!(too_soon, [], "one added since may be named by a draft on its way");
    assert_eq!(after, [AttachmentId::from("unnamed")]);
    assert_eq!(store.new_workspace_draft_file_ids().unwrap(), [AttachmentId::from("named")].into());
}

/// Renaming the tables keeps what's in them, and the record's revision still moves under its new name.
#[test]
fn whats_kept_of_the_dialog_before_its_tables_were_named_for_it_is_still_kept() {
    let dir = tempfile::tempdir().unwrap();
    let file = log_file("f1");
    let kept = NewWorkspaceDraft { text: "fix the login".to_string(), files: vec![file.clone()], ..Default::default() };
    let rows = format!(
        "UPDATE workspace_draft SET draft = '{}';
         INSERT INTO workspace_draft_files (id, name, media_type, size_bytes, created_at_ms)
             VALUES ('f1', 'f1.log', '{}', 3, 0);",
        serde_json::to_string(&kept).unwrap(),
        text_of(&file.media_type),
    );

    let store = store_from_before(dir.path(), BEFORE_NEW_WORKSPACE_DRAFT_NAMES, &rows);

    let before = store.new_workspace_draft().unwrap();
    assert_eq!(before, NewWorkspaceDraft { rev: before.rev, ..kept });
    let (after, changed) = store.set_new_workspace_draft(&typed("fix the logout", &["f1"])).unwrap();
    assert!(changed);
    assert_eq!(after.files, vec![file], "its file is still the hub's");
    assert!(after.rev > before.rev, "clients follow records by revision, so the feed would miss it otherwise");
}
