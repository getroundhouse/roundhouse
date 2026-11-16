//! A GitHub account connected through a real hub, which asks a stand-in for GitHub whose its token is, as what a
//! workspace's git uses for github.com: the hub makes it the default and sets it on the workspace's worker, whose setup
//! script's git gets it, and a block once it's disconnected. And who a provider's machine's own `gh` is logged in as,
//! which the hub records once a worker there has said.

#![cfg(test)]

use std::path::PathBuf;

use hub::{GithubAnswer, GithubStandIn};
use protocol::client::github_accounts::{GithubAccountConnect, GithubAccountDisconnect};
use protocol::model::{FoundGithubLogin, GhLogin, GhLoginWorks, GithubToken, SetupStatus};
use protocol::paths::ENV_TEST_GITHUB_API;

use crate::support::gh::{LOGGED_IN_AS, StandInGh};
use crate::support::{Stack, commit_settings, wait_for_setup, wait_until};

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
