//! The rule's rows for a repository, a workspace, the comments on its diff, and its child worktrees.

use protocol::fixtures;
use protocol::model::{BranchName, ChildStatus, ChildWorktree, DiffComment, DiffSide, RepoUrl, Rev, SetupStatus};

use super::super::Store;
use super::rule::Said;

pub(super) fn repo(store: &Store) -> Rev {
    store.get_repo(&"r".into()).unwrap().rev
}

pub(super) fn read_address(store: &Store) -> Said {
    let url = RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap();
    Said::Changed(store.read_repo_url(&"r".into(), "/repo", Some(&url)).unwrap().is_some())
}

pub(super) fn workspace(store: &Store) -> Rev {
    store.get_workspace(&"w".into()).unwrap().rev
}

pub(super) fn setup(store: &Store) -> Said {
    Said::record(&store.set_workspace_setup(&"w".into(), SetupStatus::Running).unwrap())
}

/// As archiving it records the branch its worktree was on.
pub(super) fn branch(store: &Store) -> Said {
    Said::Changed(store.set_workspace_branch(&"w".into(), &BranchName::from("alder/renamed")).unwrap())
}

pub(super) fn comment(store: &Store) -> Rev {
    store.list_comments(&"w".into()).unwrap()[0].rev
}

pub(super) fn commented(store: &Store) {
    let comment = DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "src/lib.rs".into(),
        side: DiffSide::New,
        line: 12,
        text: "why?".into(),
        created_at_ms: 1,
        rev: Rev::default(),
    };
    store.add_comment(&comment, None).unwrap();
}

pub(super) fn edit(store: &Store) -> Said {
    Said::record(&store.edit_comment(&"c".into(), "why not?").unwrap().made)
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

/// As its worker answers that it's made, which a worker asked again answers again.
pub(super) fn ready(store: &Store) -> Said {
    Said::Changed(store.child_ready(&"a".into()).unwrap().is_some())
}
