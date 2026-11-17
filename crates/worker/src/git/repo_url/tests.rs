//! Against real git, with a git config of the person's own for each test, and none of the machine's, so what this
//! machine's own rewrites doesn't count.

use std::path::Path;

use process::Environment;
use protocol::client::ErrorCode;
use protocol::worker::{PushRoute, RepoUrlReply};

use super::repo_url;
use crate::git::tests::{new_repo, run};

/// The machine's own config, as the person's global `config` alone, written in `dir`. Without the `GIT_CONFIG_*` of
/// whatever runs the tests, such as a worker's own, which gives its git credential's rewriting to what runs in a
/// workspace, so only the code under test adds that.
fn machine(dir: &Path, config: &str) -> Environment {
    let global = dir.join("person.gitconfig");
    std::fs::write(&global, config).unwrap();
    Environment::NONE.with("GIT_CONFIG_GLOBAL", global).with("GIT_CONFIG_NOSYSTEM", "1").without("GIT_CONFIG_COUNT")
}

async fn read(repo: &Path) -> RepoUrlReply {
    let dir = repo.parent().unwrap();
    repo_url(&machine(dir, ""), repo).await.unwrap()
}

/// Where it fetches from, as the person's own rewriting of addresses has it, and not where it pushes.
#[tokio::test]
async fn a_checkout_says_where_origin_fetches_from_and_none_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    run(&repo, &["remote", "add", "upstream", "https://github.com/someone/else.git"]);

    let without = read(&repo).await;
    run(&repo, &["remote", "add", "origin", "ssh://git@example.com/ada/roundhouse.git"]);
    run(&repo, &["config", "--add", "remote.origin.url", "https://example.com/second.git"]);
    run(&repo, &["remote", "set-url", "--push", "origin", "https://example.com/pushed.git"]);
    let with = read(&repo).await;
    run(&repo, &["config", "url.https://github.com/.insteadOf", "gh:"]);
    run(&repo, &["remote", "remove", "origin"]);
    run(&repo, &["remote", "add", "origin", "gh:octo/hello"]);
    let rewritten = read(&repo).await;

    assert_eq!(without, RepoUrlReply { url: None, push: None }, "another remote isn't origin");
    assert_eq!(
        with.url.map(String::from).as_deref(),
        Some("ssh://git@example.com/ada/roundhouse.git"),
        "the first, which git fetches from"
    );
    assert_eq!(rewritten.url.map(String::from).as_deref(), Some("https://github.com/octo/hello"));
}

#[tokio::test]
async fn a_credential_in_origins_address_is_cut_out_before_it_is_said() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    run(&repo, &["remote", "add", "origin", "https://ada:ghp_made_up@github.com/ada/roundhouse.git"]);

    let said = read(&repo).await;

    assert_eq!(said.url.map(String::from).as_deref(), Some("https://github.com/ada/roundhouse.git"));
}

#[tokio::test]
async fn a_directory_that_isnt_a_checkout_is_refused_rather_than_said_to_have_none() {
    let dir = tempfile::tempdir().unwrap();

    let read = repo_url(&Environment::NONE, dir.path()).await;

    assert_eq!(read.map_err(|e| e.code), Err(ErrorCode::Worker));
}

/// Where a push from a checkout whose `origin` is `url` goes, under the person's own git config `config`, and with
/// each of `push_urls` as a push address of its own.
async fn route(url: &str, push_urls: &[&str], config: &str) -> Option<PushRoute> {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    run(&repo, &["remote", "add", "origin", url]);
    for push_url in push_urls {
        run(&repo, &["config", "--add", "remote.origin.pushurl", push_url]);
    }
    repo_url(&machine(dir.path(), config), &repo).await.unwrap().push
}

/// Each kind of address git writes for GitHub, and others, as the worker's own rewriting of GitHub's SSH addresses
/// meets them: only what ends up `https://github.com/` goes through the credential.
#[tokio::test]
async fn a_push_goes_through_the_credential_only_to_https_github_com() {
    let through = [
        "https://github.com/ada/roundhouse.git",
        "HTTPS://GitHub.com/ada/roundhouse",
        "https://ada@github.com/ada/roundhouse.git",
        "git@github.com:ada/roundhouse.git",
        "ssh://git@github.com/ada/roundhouse.git",
    ];
    let around = [
        "ssh://github.com/ada/roundhouse.git",
        "ssh://git@github.com:22/ada/roundhouse.git",
        "git@ssh.github.com:ada/roundhouse.git",
        "https://ada:made_up@github.com/ada/roundhouse.git",
        "https://github.com:8443/ada/roundhouse.git",
        "http://github.com/ada/roundhouse.git",
        "https://gitlab.com/ada/roundhouse.git",
        "/srv/git/roundhouse.git",
    ];

    for url in through {
        assert_eq!(route(url, &[], "").await, Some(PushRoute::Credential), "{url}");
    }
    for url in around {
        assert_eq!(route(url, &[], "").await, Some(PushRoute::Around), "{url}");
    }
}

/// Git takes one rewrite of each address, the longest that fits, so the person's of `https://github.com/` to SSH
/// wins over the worker's of SSH to it, and pushes go round the credential with the machine's key. One of theirs
/// that ends up at `https://github.com/` goes through it.
#[tokio::test]
async fn the_persons_own_rewriting_counts_as_a_push_meets_it() {
    let to_ssh = "[url \"git@github.com:\"]\n\tinsteadOf = https://github.com/\n";
    let pushes_to_ssh = "[url \"git@github.com:\"]\n\tpushInsteadOf = https://github.com/\n";
    let to_https = "[url \"https://github.com/\"]\n\tinsteadOf = gh:\n";
    let https = "https://github.com/ada/roundhouse.git";

    let rewritten_to_ssh = route(https, &[], to_ssh).await;
    let pushed_over_ssh = route(https, &[], pushes_to_ssh).await;
    let short = route("gh:ada/roundhouse", &[], to_https).await;

    assert_eq!(rewritten_to_ssh, Some(PushRoute::Around));
    assert_eq!(pushed_over_ssh, Some(PushRoute::Around));
    assert_eq!(short, Some(PushRoute::Credential));
}

/// A `pushurl` is where git pushes, rewritten as any address is, and git pushes to every one.
#[tokio::test]
async fn a_push_goes_where_each_push_address_does() {
    let https = "https://github.com/ada/roundhouse.git";

    let over_ssh_rewritten = route(https, &["git@github.com:ada/fork.git"], "").await;
    let over_ssh = route(https, &["ssh://github.com/ada/fork.git"], "").await;
    let one_of_two = route(https, &["https://github.com/ada/fork.git", "ssh://github.com/ada/fork.git"], "").await;
    let fetching_over_ssh = route("ssh://github.com/ada/roundhouse.git", &[https], "").await;

    assert_eq!(over_ssh_rewritten, Some(PushRoute::Credential));
    assert_eq!(over_ssh, Some(PushRoute::Around));
    assert_eq!(one_of_two, Some(PushRoute::Around));
    assert_eq!(fetching_over_ssh, Some(PushRoute::Credential), "only where it pushes counts");
}
