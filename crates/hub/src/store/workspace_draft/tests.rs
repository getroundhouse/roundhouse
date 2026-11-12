use protocol::attachment::MediaType;
use protocol::model::{DEFAULT_MODEL, Effort, RepoId};

use super::*;
use crate::store::tests::store_with_session;

fn log_file(id: &str) -> Attachment {
    Attachment { id: id.into(), name: format!("{id}.log"), media_type: MediaType::Text, size_bytes: 3 }
}

fn typed(text: &str, files: &[&str]) -> WorkspaceDraftToKeep {
    WorkspaceDraftToKeep {
        repo_id: Some(RepoId::from("r")),
        text: text.to_string(),
        files: files.iter().map(|&id| id.into()).collect(),
        model: Some(DEFAULT_MODEL),
        effort: Some(Effort::High),
        plan: true,
    }
}

#[test]
fn a_hub_starts_with_nothing_kept_and_a_revision_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let draft = store.workspace_draft().unwrap();

    assert_eq!(draft, WorkspaceDraft { rev: draft.rev, ..WorkspaceDraft::default() });
    assert!(draft.rev > Rev::default(), "a record a client can tell is newer than none");
    assert_eq!(store.list_records().unwrap().workspace_draft, Some(draft));
}

#[test]
fn whats_kept_is_there_until_something_else_is_and_the_same_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.workspace_draft().unwrap().rev;

    let (kept, changed) = store.set_workspace_draft(&typed("fix the login", &[])).unwrap();
    let (again, changed_again) = store.set_workspace_draft(&typed("fix the login", &[])).unwrap();
    let (replaced, _) = store.set_workspace_draft(&typed("fix the logout", &[])).unwrap();

    assert!(changed && kept.rev > before);
    assert_eq!((kept.repo_id.as_ref(), kept.text.as_str()), (Some(&RepoId::from("r")), "fix the login"));
    assert_eq!((kept.model.clone(), kept.effort, kept.plan), (Some(DEFAULT_MODEL), Some(Effort::High), true));
    assert!(!changed_again);
    assert_eq!(again, kept, "the same revision");
    assert!(replaced.rev > kept.rev);
    assert_eq!(store.workspace_draft().unwrap(), replaced);
}

#[test]
fn its_files_are_named_as_the_hub_has_them_and_one_it_hasnt_been_given_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_workspace_draft_file(&log_file("a1")).unwrap();

    let (kept, _) = store.set_workspace_draft(&typed("", &["a1", "a2"])).unwrap();

    assert_eq!(kept.files, [log_file("a1")]);
}

#[test]
fn adding_a_file_again_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_workspace_draft_file(&log_file("a1")).unwrap();

    let again = store.add_workspace_draft_file(&Attachment { name: "other.log".into(), ..log_file("a1") }).unwrap();

    assert_eq!(again, log_file("a1"));
}

#[test]
fn a_file_is_let_go_of_only_once_the_draft_no_longer_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_workspace_draft_file(&log_file("a1")).unwrap();
    store.set_workspace_draft(&typed("", &["a1"])).unwrap();

    let while_named = store.let_go_of_workspace_draft_file(&"a1".into()).unwrap();
    store.set_workspace_draft(&typed("", &[])).unwrap();
    let once_not = store.let_go_of_workspace_draft_file(&"a1".into()).unwrap();
    let again = store.let_go_of_workspace_draft_file(&"a1".into()).unwrap();

    assert!(!while_named);
    assert!(once_not);
    assert!(!again, "nothing more to let go of");
    assert_eq!(store.workspace_draft_file(&"a1".into()).unwrap(), None);
}
