//! The name a new workspace takes, which the hub checks and refuses from what its other workspaces have and what the
//! new one's worker says git has. Which names git has is the worker's to say, as its own tests in `git::tests` have
//! it, and the daemon's `e2e/workspaces.rs` has a real worker saying so of a real repository.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::WorkspaceId;
use protocol::worker::{NamesGitHasReply, WorkerRequest, WorkerResponse};

use crate::hub::stand_in_workers::hub_whose_stand_ins_answer_first;
use crate::hub::tests::described;

/// A name that could leave the workspaces' directory, or one another workspace has, is refused, and so is one git has
/// as a branch, or as a folder of them, as the worker says, and none of them leaves anything behind. A name the hub
/// picks is a station, and where git has every station, the next it picks is one with a number, in the instance made
/// for the first, since an instance is known by its ID alone. A stand-in is its instance's worker from when it's made.
#[tokio::test]
async fn a_name_that_could_escape_or_that_a_workspace_or_git_has_is_refused_leaving_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_whose_stand_ins_answer_first(dir.path(), vec![described()], |request| {
        let WorkerRequest::NamesGitHas(_) = request else { return None };
        let stations = crate::workspace_names().iter().map(|name| (*name).to_string());
        let names = ["taken".to_string(), "folder".to_string()].into_iter().chain(stations).collect();
        Some(Ok(WorkerResponse::NamesGitHas(NamesGitHasReply { names })))
    });
    let named = |name: &str| hub.workspace_create("r".into(), Some(name.to_string()), None, None);
    let code = |refused: Result<_, ApiError>| refused.map_err(|e| e.code).err();

    let escaping = code(named("../escape").await);
    let first = named("mine").await.unwrap();
    let again = code(named("mine").await);
    let git_has = [code(named("taken").await), code(named("folder").await)];
    let made: Vec<WorkspaceId> = hub.workspace_list(true).await.unwrap().into_iter().map(|w| w.id).collect();
    let before = (asked.instances_created(), asked.instances_destroyed());
    let generated = hub.workspace_create("r".into(), None, None, None).await.unwrap();
    let after = (asked.instances_created(), asked.instances_destroyed());

    assert_eq!(escaping, Some(ErrorCode::InvalidRequest));
    assert_eq!(again, Some(ErrorCode::NameTaken), "another workspace has it");
    assert_eq!(git_has, [Some(ErrorCode::NameTaken); 2], "a branch, and a folder of branches");
    let made: Vec<_> = made.into_iter().filter(|id| *id != WorkspaceId::from("w")).collect();
    assert_eq!(made, [first.id], "nothing was left of those refused");
    let station = generated.name.strip_suffix("-2").unwrap_or_else(|| panic!("{} has no number", generated.name));
    assert!(crate::workspace_names().contains(&station), "{}", generated.name);
    assert_eq!((after.0 - before.0, after.1 - before.1), (1, 0), "one instance made, and none destroyed");
}
