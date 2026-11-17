//! A GitHub account connected through a real hub, which asks a stand-in for GitHub whose its token is, as what a
//! workspace's git uses for github.com: the hub makes it the default and sets it on the workspace's worker, whose setup
//! script's git gets it, and commits as it, and a block once it's disconnected. And who a provider's machine's own `gh`
//! is logged in as, which the hub records once a worker there has said, and whether the default can push to a
//! workspace's repository.

#![cfg(test)]

use std::path::PathBuf;

use hub::{GithubAnswer, GithubGate, GithubScript, GithubStandIn};
use protocol::client::github_accounts::{GithubAccountConnect, GithubAccountDisconnect};
use protocol::model::{
    FoundGithubLogin, GhLogin, GhLoginWorks, GithubReach, GithubRepositoryName, GithubToken, Reach, SetupStatus,
};
use protocol::paths::ENV_TEST_GITHUB_API;

use crate::support::gh::{LOGGED_IN_AS, StandInGh};
use crate::support::{Stack, commit_settings, git, wait_for_setup, wait_until};

/// Asks git for github.com's credential at once, keeping what it filled in `first`, then asks again until git can't
/// fill it, as once the hub has blocked it, keeping what git said then in `said`, or fails after five seconds or so.
const ASKS_GIT: &str = r"fill() { printf 'protocol=https\nhost=github.com\n\n' | git credential fill; }
fill > first.tmp 2>&1
mv first.tmp first
tries=0
while fill > /dev/null 2> said; do
  tries=$((tries + 1))
  [ $tries -lt 250 ] || exit 1
  sleep 0.01
done";

const TOKEN: &str = "github_pat_made_up_for_a_test";

#[test]
fn a_workspaces_git_gets_the_default_github_accounts_token_and_a_block_once_its_disconnected() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let github = runtime.block_on(GithubStandIn::start(vec![(TOKEN, GithubAnswer::user(7, "ada", None))]));
    let stack = Stack::start_with(vec![(ENV_TEST_GITHUB_API, github.api())]);
    commit_settings(&stack, &format!("[scripts]\nsetup = '''\n{ASKS_GIT}\n'''\n"));
    let client = stack.client();
    // The first account the hub has a record of becomes the default.
    let account = client.call(GithubAccountConnect { token: GithubToken::new(TOKEN), id: None }).unwrap();
    let (workspace, _) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    wait_until("the setup script's first ask", || worktree.join("first").exists());
    let first = std::fs::read_to_string(worktree.join("first")).unwrap();

    client.call(GithubAccountDisconnect { id: account.id }).unwrap();

    assert!(first.contains(&format!("username=ada\npassword={TOKEN}\n")), "{first}");
    assert_eq!(wait_for_setup(&client, &workspace.id), SetupStatus::Succeeded, "git was blocked");
    let said = std::fs::read_to_string(worktree.join("said")).unwrap();
    assert!(said.contains("Roundhouse's default GitHub account, ada, was disconnected"), "{said}");
}

/// Commits as git does on a machine with no name or email of its own, since `user.useConfigOnly` takes them only from
/// git's config, rather than guessing them from the machine, then says who the commit is by and who committed it.
const COMMITS: &str = "git -c user.useConfigOnly=true commit --allow-empty -qm 'by an agent'
git log -1 --format='%an <%ae>, %cn <%ce>' > by";

/// As GitHub's profile of the account's user says, when the hub asks whose its token is: her name, and the address
/// GitHub gives her, since she shows no email.
#[test]
fn a_workspaces_commits_are_by_the_default_github_account_on_a_machine_with_no_git_identity() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ada = GithubAnswer::user_with_profile(7, "ada", Some("Ada Lovelace"), None);
    let github = runtime.block_on(GithubStandIn::start(vec![(TOKEN, ada)]));
    let stack = Stack::start_with(vec![(ENV_TEST_GITHUB_API, github.api())]);
    commit_settings(&stack, &format!("[scripts]\nsetup = '''\n{COMMITS}\n'''\n"));
    let client = stack.client();
    client.call(GithubAccountConnect { token: GithubToken::new(TOKEN), id: None }).unwrap();

    let (workspace, _) = stack.workspace(&client);

    assert_eq!(wait_for_setup(&client, &workspace.id), SetupStatus::Succeeded, "git couldn't commit");
    let by = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("by")).unwrap();
    let ada = "Ada Lovelace <7+ada@users.noreply.github.com>";
    assert_eq!(by, format!("{ada}, {ada}\n"));
}

/// Through the real worker of the local provider, and a stand-in for `gh` on its `PATH`, to every client.
#[test]
fn a_hub_records_who_its_machines_own_gh_is_logged_in_as_once_a_worker_there_has_said() {
    let mut stack = Stack::start();
    let _gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let listed = || client.records_list().unwrap().found_github_logins;
    let before = listed();

    stack.workspace(&client);
    wait_until("a found login", || !listed().is_empty());

    assert_eq!(before, []);
    let [FoundGithubLogin { provider, login, .. }] = &listed()[..] else { panic!("{:?}", listed()) };
    let works = GhLogin::LoggedIn { username: LOGGED_IN_AS.into(), works: GhLoginWorks::Yes };
    assert_eq!((provider.as_str(), login), ("local", &works));
}

/// Through the real worker of the local provider, which reads the address of its repository's `origin`, checked with
/// the default's token against a stand-in for GitHub, to every client, with when GitHub answered. A hub that starts
/// again has it at once, as the last run's, until it has checked again, which GitHub holds back here until the test
/// has seen it. Nothing has been fetched from `origin`, so the workspace starts from the checkout's own branch, and
/// nothing reaches GitHub.
#[test]
fn a_hub_records_whether_the_default_can_push_to_a_workspaces_repository_on_github_and_when() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let refused = GithubAnswer::git_says(403, "Permission to cli/cli.git denied to ada.");
    let gate = GithubGate::closed();
    let held = GithubAnswer::After(gate.clone(), Box::new(refused.clone()));
    let github = runtime.block_on(GithubStandIn::with(GithubScript {
        users: vec![(TOKEN, GithubAnswer::user(7, "ada", None))],
        pushes: vec![("cli/cli", vec![refused, held])],
        ..GithubScript::default()
    }));
    let mut stack = Stack::start_with(vec![(ENV_TEST_GITHUB_API, github.api())]);
    let repo = stack.repo();
    git(&repo, &["remote", "add", "origin", "git@github.com:cli/cli.git"]);
    let client = stack.client();
    client.call(GithubAccountConnect { token: GithubToken::new(TOKEN), id: None }).unwrap();
    let listed = |stack: &Stack| stack.client().records_list().unwrap().github_reach;

    stack.workspace(&client);
    wait_until("the repository's reach", || !listed(&stack).is_empty());
    let [checked] = &listed(&stack)[..] else { panic!("{:?}", listed(&stack)) };
    stack.restart_hub();
    let [left] = &listed(&stack)[..] else { panic!("{:?}", listed(&stack)) };
    gate.open();
    wait_until("it checked again", || listed(&stack).first().is_some_and(|reach| !reach.stale));

    let reason = "Permission to cli/cli.git denied to ada.".to_string();
    let cli = GithubRepositoryName::named("cli/cli");
    assert_eq!(checked.reach, Reach::Refused { repository: cli, username: "ada".into(), reason });
    assert!(!checked.stale && checked.answered_at_ms.is_some(), "{checked:?}");
    let as_it_was = GithubReach { stale: true, rev: left.rev, ..checked.clone() };
    assert_eq!(left, &as_it_was, "the last run's, as it was, with when GitHub answered");
    let [again] = &listed(&stack)[..] else { panic!() };
    assert!(again.answered_at_ms > checked.answered_at_ms, "{again:?}");
}
