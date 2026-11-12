use protocol::client::ErrorCode;
use protocol::model::{CommentDraft, DiffSide};

use crate::hub::tests::hub_in;

/// As from a newer client, which knows a side of a diff this hub doesn't. Refused as it would be saved, rather than as
/// a request the hub doesn't know, which would say it keeps no comment being written at all.
#[tokio::test]
async fn a_comment_being_written_on_a_side_of_a_diff_the_hub_doesnt_know_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let newer = CommentDraft {
        path: "src/lib.rs".into(),
        side: DiffSide::Unknown,
        line: 12,
        editing: None,
        text: "why?".into(),
    };

    let refused = hub.workspace_comment_draft("w".into(), Some(newer)).await;

    assert_eq!(refused.map_err(|e| e.code).err(), Some(ErrorCode::InvalidRequest));
    let kept = hub.workspace_comment_draft("w".into(), None).await.unwrap();
    assert_eq!(kept.comment_draft, None, "nothing was kept");
}
