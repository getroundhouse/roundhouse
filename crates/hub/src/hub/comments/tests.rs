use std::time::Duration;

use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{CommentDraft, DiffSide, Workspace};

use crate::hub::tests::hub_in;

/// Long enough for the feed to hear of a change already made, on a loaded machine.
const TIMEOUT: Duration = Duration::from_secs(10);

fn writing(side: DiffSide) -> CommentDraft {
    CommentDraft { path: "src/lib.rs".into(), side, line: 12, editing: None, text: "why?".into() }
}

/// As from a newer client, which knows a side of a diff this hub doesn't. Refused as it would be saved, rather than as
/// a request the hub doesn't know, which would say it keeps no comment being written at all.
#[tokio::test]
async fn a_comment_being_written_on_a_side_of_a_diff_the_hub_doesnt_know_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let refused = hub.workspace_comment_draft("w".into(), Some(writing(DiffSide::Unknown))).await;

    assert_eq!(refused.map_err(|e| e.code).err(), Some(ErrorCode::InvalidRequest));
    let kept = hub.workspace_comment_draft("w".into(), None).await.unwrap();
    assert_eq!(kept.comment_draft, None, "nothing was kept");
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
