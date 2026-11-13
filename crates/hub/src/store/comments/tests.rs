use protocol::model::{CommentDraft, CommentId, DiffComment, DiffSide, Rev, WorkspaceId};

use crate::store::tests::store_with_session;
use crate::store::{Made, Store};

fn workspace() -> WorkspaceId {
    WorkspaceId::from("w")
}

fn writing(text: &str) -> CommentDraft {
    CommentDraft { path: "src/lib.rs".into(), side: DiffSide::New, line: 12, editing: None, text: text.to_string() }
}

/// A comment saved from `from`, as a client saves the box.
fn saved(from: &CommentDraft) -> DiffComment {
    DiffComment {
        id: "c1".into(),
        workspace_id: workspace(),
        path: from.path.clone(),
        side: from.side,
        line: from.line,
        text: from.text.clone(),
        created_at_ms: 1,
        rev: Rev::default(),
    }
}

fn being_written(store: &Store) -> Option<CommentDraft> {
    store.get_workspace(&workspace()).unwrap().comment_draft
}

/// The hub lets go of it in the same commit, so a client never sees the comment saved with the box still open, and
/// one that stops between saving and clearing doesn't bring it back to be saved twice.
#[test]
fn saving_a_comment_takes_the_one_being_written_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let (before, _) = store.set_comment_draft(&workspace(), Some(&writing("why?"))).unwrap();

    let added = store.add_comment(&saved(&writing("why?")), None).unwrap();

    let Made::New(comment) = added.made else { panic!("made before") };
    let told = added.workspace.expect("the feed is told of the workspace");
    assert_eq!(told.comment_draft, None);
    assert!(told.rev > before.rev, "clients follow records by revision, so the feed would miss it otherwise");
    assert_eq!(told, store.get_workspace(&workspace()).unwrap(), "it comes back as stored");
    assert!(comment.rev <= told.rev, "both are in one commit");
}

/// What's typed since the save was sent, or a comment written elsewhere, isn't what's being saved, so it isn't lost.
#[test]
fn a_comment_being_written_that_isnt_the_one_saved_stays() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let elsewhere = [
        writing("why? And"),
        CommentDraft { line: 13, ..writing("why?") },
        CommentDraft { path: "src/main.rs".into(), ..writing("why?") },
        CommentDraft { side: DiffSide::Old, ..writing("why?") },
        CommentDraft { editing: Some("c0".into()), ..writing("why?") },
    ];

    for (n, kept) in elsewhere.into_iter().enumerate() {
        store.set_comment_draft(&workspace(), Some(&kept)).unwrap();
        let comment = DiffComment { id: format!("c{n}").into(), ..saved(&writing("why?")) };

        let added = store.add_comment(&comment, None).unwrap();

        assert_eq!(added.workspace, None, "nothing to tell of for {kept:?}");
        assert_eq!(being_written(&store), Some(kept));
    }
}

#[test]
fn saving_a_change_to_a_comment_takes_the_one_being_written_with_it_if_it_says_whats_saved() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_comment(&saved(&writing("why?")), None).unwrap();
    let id = CommentId::from("c1");
    let changing = CommentDraft { editing: Some(id.clone()), ..writing("why not?") };
    store.set_comment_draft(&workspace(), Some(&changing)).unwrap();

    let typed_on = store.edit_comment(&id, "why").unwrap();

    assert_eq!(typed_on.workspace, None, "what's typed since stays");
    assert_eq!(being_written(&store), Some(changing));

    let edited = store.edit_comment(&id, "why not?").unwrap();

    assert_eq!(edited.made.text, "why not?");
    assert_eq!(edited.workspace.expect("the feed is told of the workspace").comment_draft, None);
    assert_eq!(being_written(&store), None);
}

/// Saving it would fail with the comment gone, so what's typed in it becomes a new comment on the same line.
#[test]
fn deleting_the_comment_being_changed_leaves_what_was_typed_as_a_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_comment(&saved(&writing("why?")), None).unwrap();
    let id = CommentId::from("c1");
    let changing = CommentDraft { editing: Some(id.clone()), ..writing("why not?") };
    store.set_comment_draft(&workspace(), Some(&changing)).unwrap();

    let deleted = store.delete_comment(&id).unwrap();

    let rev = deleted.made.expect("it was there to delete");
    let told = deleted.workspace.expect("the feed is told of the workspace");
    assert_eq!(told.comment_draft, Some(writing("why not?")));
    assert!(told.rev <= rev, "both are in one commit");
    assert_eq!(store.list_comments(&workspace()).unwrap(), vec![]);
}

#[test]
fn deleting_another_comment_leaves_the_one_being_written_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.add_comment(&saved(&writing("why?")), None).unwrap();
    let other = CommentDraft { editing: Some("c0".into()), ..writing("why not?") };
    store.set_comment_draft(&workspace(), Some(&other)).unwrap();

    let deleted = store.delete_comment(&"c1".into()).unwrap();

    assert!(deleted.made.is_some());
    assert_eq!(deleted.workspace, None);
    assert_eq!(being_written(&store), Some(other));
    assert_eq!(store.delete_comment(&"c1".into()).unwrap().workspace, None, "nothing left to delete");
}
