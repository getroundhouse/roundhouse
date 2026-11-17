use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{CommentDraft, DiffSide, RecordKey, SessionStatus, Workspace};
use protocol::prompt::Prompt;

use crate::hub::stand_in_workers::{TIMEOUT, a_chat, a_turn, hub_with_stand_in_workers};
use crate::hub::tests::hub_in;

fn writing(side: DiffSide) -> CommentDraft {
    CommentDraft { path: "src/lib.rs".into(), side, line: 12, editing: None, text: "why?".into() }
}

/// As from a newer client, which knows a side of a diff this hub doesn't. Refused as it would be saved, rather than as
/// a request the hub doesn't know, which would say it keeps no comment being written at all.
#[tokio::test]
async fn a_comment_or_one_being_written_on_a_side_of_a_diff_the_hub_doesnt_know_is_refused_and_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let being_written = hub.workspace_comment_draft("w".into(), Some(writing(DiffSide::Unknown))).await;
    let comment = hub.comment_add("w".into(), "a.txt".into(), DiffSide::Unknown, 1, "Why?".into(), None).await;

    assert_eq!(being_written.map_err(|e| e.code).err(), Some(ErrorCode::InvalidRequest), "one being written");
    let kept = hub.workspace_comment_draft("w".into(), None).await.unwrap();
    assert_eq!(kept.comment_draft, None, "nothing was kept");
    assert_eq!(comment.map_err(|e| e.code).err(), Some(ErrorCode::InvalidRequest), "a comment");
    assert!(hub.comment_list("w".into()).await.unwrap().is_empty(), "no comment was saved");
}

/// So every client puts the box away as the comment shows, rather than waiting on the client that saved it.
#[tokio::test]
async fn saving_the_comment_being_written_tells_the_feed_it_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let kept = writing(DiffSide::New);
    hub.workspace_comment_draft("w".into(), Some(kept.clone())).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.comment_add("w".into(), kept.path, kept.side, kept.line, kept.text, None).await.unwrap();

    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::Workspace(workspace)) = feed.recv().await {
                return workspace;
            }
        }
    });
    let workspace: Workspace = told.await.expect("the feed never told of the workspace");
    assert_eq!(workspace.comment_draft, None);
}

/// Its copy is newer, so clients can tell which is, and an empty one, or one for a comment that's gone, is refused.
#[tokio::test]
async fn a_comment_is_edited_in_place_and_not_once_its_gone() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let add = hub.comment_add("w".into(), "a.txt".into(), DiffSide::New, 1, "Rename this".into(), None);
    let added = add.await.unwrap();

    let edited = hub.comment_edit(added.id.clone(), "Rename this to `count`".into()).await.unwrap();
    let empty = hub.comment_edit(added.id.clone(), "  ".into()).await.map_err(|e| e.code);
    let listed = hub.comment_list("w".into()).await.unwrap();
    hub.comment_delete(added.id.clone()).await.unwrap();
    let gone = hub.comment_edit(added.id.clone(), "Too late".into()).await.map_err(|e| e.code);

    assert_eq!((&edited.id, edited.line, edited.text.as_str()), (&added.id, 1, "Rename this to `count`"));
    assert!(edited.rev > added.rev, "so clients can tell which copy is newer");
    assert_eq!(empty.err(), Some(ErrorCode::InvalidRequest), "an empty edit");
    assert_eq!(listed, [edited]);
    assert_eq!(gone.err(), Some(ErrorCode::NotFound), "an edit of one that's gone");
    assert!(hub.comment_list("w".into()).await.unwrap().is_empty(), "editing doesn't bring it back");
}

/// Each change since a list is newer than it, as the feed tells of it, and a list after is at least as new, so a
/// client that follows the feed from a list keeps the newer copy of each record: a chat's, as its turn moves it, and
/// a comment's, as it's added and removed.
#[tokio::test]
async fn a_list_and_the_feed_agree_on_which_copy_is_newer() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let mut feed = hub.subscribe_feed();
    let before = hub.records_list().await.unwrap();

    a_turn(&hub, &asked, &chat.id, Prompt::new("hello"), None).await;
    let add = hub.comment_add(workspace.id.clone(), "a.txt".into(), DiffSide::New, 1, "Rename this".into(), None);
    let comment = add.await.unwrap();
    hub.comment_delete(comment.id.clone()).await.unwrap();
    let notices: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok()).collect();
    let after = hub.records_list().await.unwrap();

    let newest_session = notices.iter().rev().find_map(|notice| {
        let HubNotice::Session(session) = notice else { return None };
        (session.id == chat.id && session.rev > before.rev).then_some(session)
    });
    let listed = after.sessions.iter().find(|session| session.id == chat.id).unwrap();
    assert!(listed.rev >= newest_session.expect("the feed told of the chat's turn").rev);
    assert_eq!(listed.status, SessionStatus::Idle);
    let added = notices.iter().find_map(|notice| {
        let HubNotice::Comment(told) = notice else { return None };
        (told.id == comment.id).then_some(told)
    });
    assert_eq!(added, Some(&comment));
    let Some(HubNotice::Removed { record, rev }) = notices.last() else { panic!("the comment's removal: {notices:?}") };
    assert_eq!(record, &RecordKey::Comment { id: comment.id.clone() });
    assert!(before.rev < comment.rev && comment.rev < *rev && *rev <= after.rev);
    assert!(after.comments.is_empty());
}
