use protocol::client::ErrorCode;
use protocol::fixtures::workspace;
use protocol::model::{
    Apart, Branch, BranchName, Branches, CommitId, GitHubRepository, GithubRepositoryName, Head, IssueNumber, Local,
    PrState, PullRequestLink, PullRequestLinked, PullRequestNumber, PullRequestSelection, RemoteName, RepoUrl,
    Upstream, Workspace,
};

use super::judging::{Wanted, chosen, found_on_another_branch, local, wanted};
use super::scopes::scopes;
use crate::hub::pull_requests::remote::Waited;
use github::{Headed, HeadsAsk};

fn name(name: &str) -> GithubRepositoryName {
    GithubRepositoryName::try_from(name.to_string()).unwrap()
}

fn url(url: &str) -> RepoUrl {
    RepoUrl::try_from(url.to_string()).unwrap()
}

/// `birch` at `tip`, pushed to `remote`'s `birch`, at `pushed`, whose address is `address`, one commit ahead.
fn birch(tip: &str, upstream: Option<(&str, Option<&str>, &str)>) -> Branch {
    let upstream = upstream.map(|(remote, address, pushed)| Upstream {
        remote: RemoteName::from(remote),
        name: BranchName::from("birch"),
        tip: Some(CommitId::from(pushed)),
        address: address.map(url),
        apart: Some(Apart { ahead: 1, behind: 0 }),
    });
    Branch { name: BranchName::from("birch"), tip: CommitId::from(tip), upstream }
}

/// A worktree's branches with `birch` checked out, as `branch` says it is.
fn on(branch: Branch) -> Branches {
    Branches { head: Head::Branch { name: branch.name.clone() }, had: vec![branch] }
}

fn project() -> RepoUrl {
    url("https://github.com/example/project.git")
}

#[test]
fn a_branch_pushed_to_github_is_asked_after_in_the_repository_its_work_merges_into() {
    let birch_on = || Some(BranchName::from("birch"));

    let unpushed = wanted(Some(&on(birch("a", None))), Some(&project()));
    let to_itself = wanted(Some(&on(birch("a", Some((".", None, "b"))))), Some(&project()));
    let detached = Branches { head: Head::Detached { commit: CommitId::from("a") }, had: Vec::new() };
    let origin = wanted(
        Some(&on(birch("a", Some(("origin", Some("git@github.com:example/project.git"), "b"))))),
        Some(&project()),
    );
    let fork =
        wanted(Some(&on(birch("a", Some(("ada", Some("https://github.com/ada/project.git"), "b"))))), Some(&project()));
    let unsaid = wanted(Some(&on(birch("a", Some(("origin", None, "b"))))), Some(&project()));
    // As `git switch -c birch origin/main` leaves it.
    let mut tracking = birch("a", Some(("origin", Some("https://github.com/example/project.git"), "b")));
    tracking.upstream.as_mut().unwrap().name = BranchName::from("main");
    let tracking = wanted(Some(&on(tracking)), Some(&project()));

    assert_eq!(unpushed, Wanted::Nothing { on: birch_on() });
    assert_eq!(to_itself, Wanted::Nothing { on: birch_on() });
    assert_eq!(tracking, Wanted::Nothing { on: birch_on() }, "taken from main, and pushed nowhere yet");
    assert_eq!(wanted(Some(&detached), Some(&project())), Wanted::Nothing { on: None });
    let heads = |from: &str| HeadsAsk {
        repository: name("example/project"),
        from: name(from),
        branch: BranchName::from("birch"),
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

    let elsewhere = wanted(Some(&pushed), Some(&url("https://gitlab.com/example/project.git")));
    let unknown = wanted(Some(&pushed), None);
    let pushed_elsewhere = wanted(Some(&to_gitlab), Some(&project()));

    for wanted in [elsewhere, unknown, pushed_elsewhere] {
        let Wanted::Unreadable { on, error } = wanted else { panic!("{wanted:?}") };
        assert_eq!((on.as_str(), error.code), ("birch", ErrorCode::Unavailable));
    }
}

/// One found on a branch other than the one checked out isn't the workspace's now, but none found on no branch, as
/// where `gh` can't work at all, says nothing of any branch, and nor do branches that aren't known.
#[test]
fn only_what_was_found_on_a_branch_can_be_another_branchs() {
    let link = PullRequestLink {
        repository: name("o/r"),
        number: IssueNumber::try_from(7).unwrap(),
        url: "https://github.com/o/r/pull/7".into(),
        uncommitted: 0,
    };
    let next = Branches { head: Head::Branch { name: BranchName::from("next") }, had: Vec::new() };
    let found = |link: Option<PullRequestLink>, branch: Option<&str>| Workspace {
        pull_request_branch: branch.map(BranchName::from),
        pull_request_link: Some(PullRequestLinked { link }),
        ..workspace("w")
    };
    let elsewhere = |link, branch| found_on_another_branch(&found(link, branch), Some(&next));

    assert!(elsewhere(Some(link.clone()), Some("aspen")));
    assert!(elsewhere(None, Some("aspen")), "the branch on now may have one");
    assert!(elsewhere(Some(link.clone()), None), "found before branches were kept");
    assert!(!elsewhere(Some(link.clone()), Some("next")));
    assert!(!elsewhere(None, None), "as where gh can't work");
    assert!(!found_on_another_branch(&found(Some(link), Some("aspen")), None), "nothing known is checked out");
}

fn headed(number: u64, state: PrState, head: &str) -> Headed {
    Headed {
        into: name("example/project"),
        number: PullRequestNumber::try_from(number).unwrap(),
        url: format!("https://github.com/example/project/pull/{number}"),
        state,
        head: CommitId::from(head),
    }
}

/// The address of pull request `number` of `example/project`, as the workspace's record links to it.
fn known(number: u64) -> String {
    format!("https://github.com/example/project/pull/{number}")
}

#[test]
fn the_workspaces_is_the_open_one_or_the_one_it_had_or_one_at_a_commit_the_branch_has() {
    let branch = birch("tip", Some(("origin", None, "pushed")));
    let number =
        |headed: &[Headed], known: Option<&str>| chosen(headed, known, &branch).map(|found| found.number.get());
    let merged = |n, head| headed(n, PrState::Merged, head);
    let elsewhere = Headed { url: "https://github.com/example/other/pull/7".into(), ..merged(7, "else") };

    let open = number(&[merged(9, "old"), headed(8, PrState::Open, "else")], None);
    let had = number(&[merged(9, "old"), merged(7, "else")], Some(&known(7)));
    let had_elsewhere = number(&[merged(9, "old"), elsewhere], Some(&known(7)));
    let at_the_push = number(&[merged(9, "old"), merged(7, "pushed")], None);
    let at_the_tip = number(&[headed(6, PrState::Closed, "tip")], None);
    let a_name_used_before = number(&[merged(9, "old")], None);

    assert_eq!(open, Some(8));
    assert_eq!(had, Some(7));
    assert_eq!(had_elsewhere, None, "the same number in another repository");
    assert_eq!(at_the_push, Some(7));
    assert_eq!(at_the_tip, Some(6));
    assert_eq!(a_name_used_before, None, "merged from a branch of the same name before this one");
}

#[test]
fn the_worktree_stands_against_its_upstream_once_the_pull_requests_head_is_there() {
    let branch = birch("tip", Some(("origin", None, "pushed")));
    let counts = |local: Local| (local.uncommitted, local.ahead, local.behind);

    assert_eq!(counts(local(&CommitId::from("tip"), &branch, 2)), (2, 0, 0));
    assert_eq!(counts(local(&CommitId::from("pushed"), &branch, 0)), (0, 1, 0));
    assert_eq!(counts(local(&CommitId::from("someone-elses"), &branch, 0)), (0, 1, 1), "a commit not fetched");
}

/// What waits watch of example/project, on `host`.
fn waited(host: &str, numbers: &[u64]) -> Waited {
    let numbers: Vec<_> = numbers.iter().map(|n| PullRequestNumber::try_from(*n).unwrap()).collect();
    let url = format!("https://{host}/example/project");
    let repository = GitHubRepository { url, name_with_owner: "example/project".to_string() };
    Waited { repository, selection: PullRequestSelection::try_from(numbers).unwrap() }
}

/// The workspace's own #7 is read in the ask of the waits that watch it, however its name is written, so the bar and
/// those waits go by one reading of it, and the waits on #9, and those of a repository elsewhere, go apart.
#[test]
fn the_workspaces_own_pull_request_is_read_with_the_waits_that_watch_it() {
    let seven = PullRequestNumber::try_from(7).unwrap();
    let stack = waited("github.com", &[6, 7]);
    let other = waited("github.com", &[9]);
    let elsewhere = waited("ghe.example.com", &[7]);

    let (asked, unreadable) =
        scopes(Some((name("Example/Project"), seven)), vec![stack.clone(), other.clone(), elsewhere.clone()]);

    let asks: Vec<_> =
        asked.iter().map(|scope| (scope.ask.selection.numbers().len(), scope.own, scope.waited.clone())).collect();
    assert_eq!(asks, [(2, Some(seven), Some(stack.repository)), (1, None, Some(other.repository.clone()))]);
    assert_eq!(unreadable.iter().map(|read| read.repository.clone()).collect::<Vec<_>>(), [elsewhere.repository]);
    let (alone, _) = scopes(Some((name("example/project"), seven)), vec![other]);
    assert_eq!(
        alone.iter().map(|scope| (scope.own, scope.waited.is_some())).collect::<Vec<_>>(),
        [(None, true), (Some(seven), false)]
    );
}
