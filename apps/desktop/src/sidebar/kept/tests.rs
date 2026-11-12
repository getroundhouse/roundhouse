use protocol::model::{InstanceId, Rev, SetupStatus, TitleSource, Workspace, WorkspaceStatus};

use super::Kept;

fn workspace(name: &str) -> Workspace {
    Workspace {
        id: name.into(),
        repo_id: "r".into(),
        name: name.to_string(),
        branch: name.to_string(),
        base_ref: None,
        path: None,
        status: WorkspaceStatus::Ready,
        provider: "local".to_string(),
        instance_id: InstanceId::from("local"),
        created_at_ms: 0,
        archived_at_ms: None,
        user_active_at_ms: None,
        setup: SetupStatus::NotStarted,
        port_base: 0,
        rev: Rev::default(),
        title: None,
        title_source: TitleSource::default(),
        pull_request: None,
    }
}

#[test]
fn the_workspace_being_made_is_the_one_that_wasnt_there_as_it_was_asked_for() {
    let workspaces = ["alder", "birch", "cedar"].map(workspace);
    let mut kept = Kept::default();
    assert!(!kept.made_is_listed(&workspaces, &[0, 1, 2]), "nothing was asked for");

    kept.making_among(&workspaces[..2]);

    assert!(!kept.made_is_listed(&workspaces, &[0, 1]));
    assert!(kept.made_is_listed(&workspaces, &[0, 1, 2]));
}
