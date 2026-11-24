use protocol::fixtures;
use protocol::model::{Branch, BranchName, Branches, ChildWorktree, Head, WorkName, Workspace};

use super::{Had, NotKnown, Try, child_path, new_branches, places_of_its};

/// The workspace `alder`, on `branch`.
fn alder(branch: &str) -> Workspace {
    Workspace { name: "alder".into(), branch: branch.into(), ..fixtures::workspace("w") }
}

/// Branches whose worktree is on the first of `names` and has had them all.
fn has_had(names: &[&str]) -> Branches {
    let had = |name: &&str| Branch { name: BranchName::from(*name), tip: "abc".into(), upstream: None };
    Branches { head: Head::Branch { name: BranchName::from(names[0]) }, had: names.iter().map(had).collect() }
}

fn first(tries: Result<Option<Vec<Try>>, NotKnown>) -> Result<Option<(String, String)>, NotKnown> {
    tries.map(|tries| tries.map(|tries| (tries[0].branch.to_string(), tries[0].path.clone())))
}

#[test]
fn a_child_s_path_is_beside_the_workspace_s_own_and_spelled_as_it_is() {
    assert_eq!(child_path("/w/repo/holly", "holly", "x"), "/w/repo/.children/holly/x");
    assert_eq!(child_path("/w/repo/holly/", "holly", "x"), "/w/repo/.children/holly/x", "not inside it");
    assert_eq!(child_path(r"C:\w\repo\holly", "holly", "x"), r"C:\w\repo\.children\holly\x");
    assert_eq!(child_path("holly", "holly", "x"), ".children/holly/x", "beside it, wherever it is");
}

#[test]
fn a_new_branch_goes_in_the_workspace_s_folder_and_a_taken_name_s_next_has_a_number() {
    let name = WorkName::try_from("try".to_string()).unwrap();

    let tries = new_branches(&alder("alder/start"), Some(&has_had(&["alder/start"])), "/ws/alder", &name).unwrap();

    let named: Vec<_> = tries.iter().take(3).map(|t| (t.branch.as_str(), t.path.as_str())).collect();
    assert_eq!(
        named,
        [
            ("alder/try", "/ws/.children/alder/try"),
            ("alder/try-2", "/ws/.children/alder/try-2"),
            ("alder/try-3", "/ws/.children/alder/try-3"),
        ]
    );
}

/// A workspace made before its branches went in a folder, beside whose branch `alder` git can't have `alder/…`, names
/// its branches beside it, while it has that branch or has had it. Its worktrees go in the same folder either way.
/// Where its record is on another branch, and what its worktree has had isn't known, which can't be told.
#[test]
fn an_old_workspace_names_its_branches_beside_it() {
    let name = WorkName::try_from("try".to_string()).unwrap();

    let on_its_branch = new_branches(&alder("alder"), None, "/ws/alder", &name).unwrap();
    let since = new_branches(&alder("fix"), Some(&has_had(&["fix", "alder"])), "/ws/alder", &name).unwrap();
    let not_known = new_branches(&alder("fix"), None, "/ws/alder", &name);

    let beside = Try { branch: "alder--try".into(), path: "/ws/.children/alder/try".into() };
    assert_eq!((&on_its_branch[0], &since[0]), (&beside, &beside));
    assert_eq!(not_known, Err(NotKnown));
}

/// Each way a branch that exists is one of the workspace's, and its directory is one name, so it can't be inside
/// another worktree's.
#[test]
fn a_branch_that_exists_is_the_workspace_s_by_its_folder_or_by_a_worktree_having_had_it() {
    let workspace = alder("renamed");
    let own = has_had(&["alder/start", "had-here"]);
    // Its branches don't say it has had the one it was made on, as before a worker has read them.
    let other = ChildWorktree { branch: "made-on".into(), ..fixtures::child_worktree("c") };
    let others = [(other, Some(has_had(&["moved-to", "had-there"])))];
    let had = Had { own: Some(&own), others: &others };
    let place = |branch: &str| first(places_of_its(&workspace, &had, "/ws/alder", &branch.into())).unwrap();
    let at = |work: &str| format!("/ws/.children/alder/{work}");

    assert_eq!(place("alder/x/y"), Some(("alder/x/y".into(), at("x-y"))), "in its folder, as one name");
    assert_eq!(place("renamed"), Some(("renamed".into(), at("renamed"))), "the one its record says it's on");
    assert_eq!(place("had-here"), Some(("had-here".into(), at("had-here"))), "one its own worktree has had");
    assert_eq!(place("made-on"), Some(("made-on".into(), at("made-on"))), "one another of its worktrees was made on");
    assert_eq!(place("had-there"), Some(("had-there".into(), at("had-there"))), "one another has had");
    assert_eq!(place("fix/login"), None, "nobody's");
    assert_eq!(place("main"), None);
    assert_eq!(place("alder--x"), None, "beside it is only an older workspace's, and may be another workspace's");
}

/// What isn't known of a worktree's branches is never taken for none: a branch that's the workspace's by what's known
/// is still its, and one that isn't can't be told to be nobody's.
#[test]
fn a_branch_is_the_workspace_s_only_by_what_s_known_and_otherwise_cant_be_told() {
    let workspace = alder("renamed");
    let other = ChildWorktree { branch: "made-on".into(), ..fixtures::child_worktree("c") };
    let others = [(other, None)];
    let had = Had { own: None, others: &others };
    let place = |branch: &str| first(places_of_its(&workspace, &had, "/ws/alder", &branch.into()));

    assert!(matches!(place("alder/x"), Ok(Some(_))), "in its folder");
    assert!(matches!(place("renamed"), Ok(Some(_))), "the one its record says");
    assert!(matches!(place("made-on"), Ok(Some(_))), "one another was made on");
    assert_eq!(place("main"), Err(NotKnown), "either worktree may have had it");
    assert_eq!(place("alder--x"), Err(NotKnown), "its own may have had `alder`, which puts its branches beside it");
}

#[test]
fn an_old_workspace_s_branches_beside_it_are_its_too() {
    let had = Had { own: None, others: &[] };
    let place = |branch: &str| first(places_of_its(&alder("alder"), &had, "/ws/alder", &branch.into())).unwrap();

    assert_eq!(place("alder--x"), Some(("alder--x".into(), "/ws/.children/alder/x".into())));
    assert_eq!(place("alder/x"), Some(("alder/x".into(), "/ws/.children/alder/x".into())));
}
