use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::model::{AttachmentId, WorkspaceDraftToKeep};

use crate::hub::tests::hub_in;

/// A hub that stopped mid-write leaves half a file, and one whose row went without its bytes leaves those. Nothing
/// could ask for either.
#[tokio::test]
async fn bytes_on_disk_with_no_file_of_the_dialogs_are_let_go_of_as_the_hub_starts() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let bytes = AttachmentBytes::new(b"log".to_vec());
    hub.workspace_draft_file_add("named".into(), "a.log".into(), MediaType::Text, bytes).await.unwrap();
    let files = dir.path().join("attachments").join("workspace-draft.files");
    for stray in ["stray", "named.7.partial"] {
        std::fs::write(files.join(stray), b"log").unwrap();
    }
    let draft = WorkspaceDraftToKeep { files: vec![AttachmentId::from("named")], ..WorkspaceDraftToKeep::default() };
    hub.workspace_draft_keep(draft).await.unwrap();

    hub.tidy_draft_files().await;

    let mut left: Vec<_> = std::fs::read_dir(&files).unwrap().map(|e| e.unwrap().file_name()).collect();
    left.sort();
    assert_eq!(left, ["named"]);
    assert_eq!(hub.workspace_draft_file_read("named".into()).await.unwrap().into_vec(), b"log");
}
