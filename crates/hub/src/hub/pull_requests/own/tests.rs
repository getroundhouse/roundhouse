use protocol::client::ErrorCode;
use protocol::fixtures::workspace;
use protocol::model::{
    Apart, Branch, BranchName, Branches, CommitId, GithubRepositoryName, Head, Local, PrState, PullRequestNumber,
    RemoteName, RepoUrl, Upstream, Workspace,
};

use super::judging::{Wanted, chosen, local, wanted};
use crate::github::{Headed, HeadsAsk};

fn name(name: &str) -> GithubRepositoryName {
    GithubRepositoryName::try_from(name.to_string()).unwrap()
}

fn url(url: &str) -> RepoUrl {
    RepoUrl::try_from(url.to_string()).unwrap()
}

/// `birch` at `tip`, pushed to `remote`'s `birch/fix`, at `pushed`, whose address is `address`, one commit ahead.
fn birch(tip: &str, upstream: Option<(&str, Option<&str>, &str)>) -> Branch {
    let upstream = upstream.map(|(remote, address, pushed)| Upstream {
        remote: RemoteName::from(remote),
        name: BranchName::from("birch/fix"),
        tip: Some(CommitId::from(pushed)),
        address: address.map(url),
        apart: Some(Apart { ahead: 1, behind: 0 }),
    });
    Branch { name: BranchName::from("birch"), tip: CommitId::from(tip), upstream }
}

/// A workspace with `birch` checked out, as `branch` says it is.
fn on(branch: Branch) -> Workspace {
    let branches = Branches { head: Head::Branch { name: branch.name.clone() }, had: vec![branch] };
    Workspace { branches: Some(branches), ..workspace("w") }
}

fn project() -> RepoUrl {
    url("https://github.com/example/project.git")
}

#[test]
fn a_branch_pushed_to_github_is_asked_after_in_the_repository_its_work_merges_into() {
    let birch_on = || Some(BranchName::from("birch"));

    let unpushed = wanted(&on(birch("a", None)), Some(&project()));
    let to_itself = wanted(&on(birch("a", Some((".", None, "b")))), Some(&project()));
    let detached = Workspace {
        branches: Some(Branches { head: Head::Detached { commit: CommitId::from("a") }, had: Vec::new() }),
        ..workspace("w")
    };
    let origin =
        wanted(&on(birch("a", Some(("origin", Some("git@github.com:example/project.git"), "b")))), Some(&project()));
    let fork =
        wanted(&on(birch("a", Some(("ada", Some("https://github.com/ada/project.git"), "b")))), Some(&project()));
    let unsaid = wanted(&on(birch("a", Some(("origin", None, "b")))), Some(&project()));

    assert_eq!(unpushed, Wanted::Nothing { on: birch_on() });
    assert_eq!(to_itself, Wanted::Nothing { on: birch_on() });
    assert_eq!(wanted(&detached, Some(&project())), Wanted::Nothing { on: None });
    let heads = |from: &str| HeadsAsk {
        repository: name("example/project"),
        from: name(from),
        branch: BranchName::from("birch/fix"),
    };
    assert_eq!(origin, Wanted::Heads { on: BranchName::from("birch"), ask: heads("example/project") });
    assert_eq!(fork, Wanted::Heads { on: BranchName::from("birch"), ask: heads("ada/project") });
    assert_eq!(
        unsaid,
        Wanted::Heads { on: BranchName::from("birch"), ask: heads("example/project") },
        "as the repository's own"
    );
}

#[test]
fn a_repository_or_a_remote_elsewhere_than_github_has_none_to_read() {
    let pushed = on(birch("a", Some(("origin", None, "b"))));
    let to_gitlab = on(birch("a", Some(("lab", Some("https://gitlab.com/ada/project.git"), "b"))));

    let elsewhere = wanted(&pushed, Some(&url("https://gitlab.com/example/project.git")));
    let unknown = wanted(&pushed, None);
    let pushed_elsewhere = wanted(&to_gitlab, Some(&project()));

    for wanted in [elsewhere, unknown, pushed_elsewhere] {
        let Wanted::Unreadable { on, error } = wanted else { panic!("{wanted:?}") };
        assert_eq!((on.as_str(), error.code), ("birch", ErrorCode::Unavailable));
    }
}

fn headed(number: u64, state: PrState, head: &str) -> Headed {
    Headed { number: PullRequestNumber::try_from(number).unwrap(), state, head: CommitId::from(head) }
}

fn known(number: u64) -> protocol::model::PullRequest {
    protocol::model::PullRequest { number, state: PrState::Merged, ..crate::hub::tests::open() }
}

#[test]
fn the_workspaces_is_the_open_one_or_the_one_it_had_or_one_at_a_commit_the_branch_has() {
    let branch = birch("tip", Some(("origin", None, "pushed")));
    let number = |found: Option<PullRequestNumber>| found.map(PullRequestNumber::get);
    let merged = |n, head| headed(n, PrState::Merged, head);

    let open = chosen(&[merged(9, "old"), headed(8, PrState::Open, "else")], None, &branch);
    let had = chosen(&[merged(9, "old"), merged(7, "else")], Some(&known(7)), &branch);
    let at_the_push = chosen(&[merged(9, "old"), merged(7, "pushed")], None, &branch);
    let at_the_tip = chosen(&[headed(6, PrState::Closed, "tip")], None, &branch);
    let a_name_used_before = chosen(&[merged(9, "old")], None, &branch);

    assert_eq!(number(open), Some(8));
    assert_eq!(number(had), Some(7));
    assert_eq!(number(at_the_push), Some(7));
    assert_eq!(number(at_the_tip), Some(6));
    assert_eq!(number(a_name_used_before), None, "merged from a branch of the same name before this one");
}

#[test]
fn the_worktree_stands_against_its_upstream_once_the_pull_requests_head_is_there() {
    let branch = birch("tip", Some(("origin", None, "pushed")));
    let counts = |local: Local| (local.uncommitted, local.ahead, local.behind);

    assert_eq!(counts(local(&CommitId::from("tip"), &branch, 2)), (2, 0, 0));
    assert_eq!(counts(local(&CommitId::from("pushed"), &branch, 0)), (0, 1, 0));
    assert_eq!(counts(local(&CommitId::from("someone-elses"), &branch, 0)), (0, 1, 1), "a commit not fetched");
}
