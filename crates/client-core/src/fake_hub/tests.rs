//! That a client reads from the fake hub what its world holds, and is refused what would change it.

use protocol::client::ErrorCode;
use protocol::model::SessionStatus;

use super::{World, serve};
use crate::{Client, ClientError};

#[test]
fn a_client_reads_what_the_world_holds_and_is_refused_a_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    let mut world = World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let chat = world.chat(&workspace, SessionStatus::Idle, Vec::new());
    serve(&socket, world);
    let client = Client::connect(&socket, "test").unwrap();

    let records = client.records_list().unwrap();
    let added = client.repo_add("/src/birch");

    assert_eq!(records.repos.iter().map(|repo| repo.name.as_str()).collect::<Vec<_>>(), ["aspen"]);
    assert_eq!(records.workspaces.iter().map(|workspace| &workspace.id).collect::<Vec<_>>(), [&workspace]);
    assert_eq!(records.sessions.iter().map(|session| &session.id).collect::<Vec<_>>(), [&chat]);
    assert!(matches!(added, Err(ClientError::Api(refused)) if refused.code == ErrorCode::Unsupported));
    assert_eq!(client.records_list().unwrap().repos.len(), 1, "and nothing was added");
}
