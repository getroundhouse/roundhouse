use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::client::ErrorCode;
use protocol::model::{AttachmentId, NewWorkspaceDraftToKeep, RepoId};

use crate::attachment_files::FileFor;
use crate::hub::tests::hub_in;
use crate::store::{AddedBy, now_ms};

fn in_the_repo(files: &[&str]) -> NewWorkspaceDraftToKeep {
    NewWorkspaceDraftToKeep {
        repo_id: Some(RepoId::from("r")),
        text: "fix the login".to_string(),
        files: files.iter().map(|&id| id.into()).collect(),
        ..NewWorkspaceDraftToKeep::default()
    }
}

/// Each is refused before a workspace is made, so a client can give it back to the dialog, which loses nothing, and
/// leaves no workspace behind without its prompt.
#[tokio::test]
async fn what_cant_be_made_is_refused_before_anything_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let before = hub.workspace_list(true).await.unwrap();
    let nowhere = NewWorkspaceDraftToKeep { repo_id: None, ..in_the_repo(&[]) };
    let too_many: Vec<String> = (0..=protocol::attachment::MAX_PER_PROMPT).map(|n| format!("f{n}")).collect();
    let too_many = in_the_repo(&too_many.iter().map(String::as_str).collect::<Vec<_>>());
    let refusals = [
        (nowhere, ErrorCode::InvalidRequest),
        (too_many, ErrorCode::InvalidRequest),
        (in_the_repo(&["not-the-dialogs"]), ErrorCode::NotFound),
    ];

    for (n, (made_of, code)) in refusals.into_iter().enumerate() {
        let refused = hub.new_workspace_create(made_of, format!("make {n}").into()).await;

        assert_eq!(refused.map_err(|e| e.code).err(), Some(code), "the request {n}");
    }
    assert_eq!(hub.workspace_list(true).await.unwrap(), before, "no workspace was made");
}

/// A file the dialog has had for long isn't let go of while a workspace is made from it, should the dialog stop naming
/// it meanwhile. It waits as long again once the hub has found it, even if making the workspace then fails, as it does
/// here, with no such repository.
#[tokio::test]
async fn the_dialogs_files_wait_as_long_again_once_a_workspace_is_made_from_them() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let data = AttachmentBytes::new(b"a line\n".to_vec());
    hub.file_add(FileFor::NewWorkspace, "f1".into(), "notes.txt".into(), MediaType::Text, data, AddedBy::Client)
        .await
        .unwrap();
    let asked_at = now_ms() + 1;
    while now_ms() < asked_at {
        std::thread::yield_now();
    }

    let _ = hub.new_workspace_create(in_the_repo(&["f1"]), "make".into()).await;

    let let_go_of = hub.blocking(move |store| store.let_go_of_unnamed_new_workspace_draft_files(asked_at)).await;
    assert_eq!(let_go_of.unwrap(), Vec::<AttachmentId>::new(), "added before it was asked for, and renewed since");
}
