//! The rule's rows for a repository, a workspace, and its child worktrees.

use protocol::fixtures;
use protocol::model::{BranchName, Branches, ChildStatus, ChildWorktree, RepoUrl, Rev};

use super::super::Store;
use super::rule::Said;

pub(super) fn repo(store: &Store) -> Rev {
    store.get_repo(&"r".into()).unwrap().rev
}

pub(super) fn read_address(store: &Store) -> Said {
    let url = RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap();
    Said::Changed(store.read_repo_url(&"r".into(), "/repo", Some(&url), None).unwrap().is_some())
}

pub(super) fn workspace(store: &Store) -> Rev {
    store.get_workspace(&"w".into()).unwrap().rev
}

pub(super) fn older_build(store: &Store) -> Said {
    Said::Changed(store.set_workspace_worker_build(&"w".into(), Some("0.9.0")).unwrap().is_some())
}

pub(super) fn on_older_build(store: &Store) {
    older_build(store);
}

pub(super) fn own_build(store: &Store) -> Said {
    Said::Changed(store.set_workspace_worker_build(&"w".into(), None).unwrap().is_some())
}

pub(super) fn child(store: &Store) -> Rev {
    store.child_at(&"w".into(), "/c/x").unwrap().unwrap().rev
}

pub(super) fn made_child(store: &Store) {
    let child = ChildWorktree {
        path: "/c/x".into(),
        branch: BranchName::from("alder/x"),
        created_at_ms: 1,
        status: ChildStatus::Creating,
        ..fixtures::child_worktree("a")
    };
    store.insert_child(&child, &"k1".into()).unwrap();
}

pub(super) fn branches(store: &Store) -> Said {
    let on = Branches { head: protocol::model::Head::Branch { name: BranchName::from("alder/x") }, had: vec![] };
    Said::Changed(store.set_child_branches(&"w".into(), "/c/x", &on).unwrap().is_some())
}
