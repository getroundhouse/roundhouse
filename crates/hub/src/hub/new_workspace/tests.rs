use protocol::client::ErrorCode;
use protocol::model::{NewWorkspaceDraftToKeep, RepoId};

use crate::hub::tests::hub_in;

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
