//! Against real git, with a git config of the person's own for each test, and none of the machine's, so what this
//! machine's own rewrites doesn't count.

use std::path::Path;

use process::Environment;
use protocol::client::ErrorCode;
use protocol::worker::RepoUrlReply;

use super::repo_url;
use crate::git::tests::{new_repo, run};
use crate::testing::given;

/// The machine's own config, as the person's global `config` alone, written in `dir`, in what a test gives the worker's
/// git, which has none of the `GIT_CONFIG_*` of whatever runs the tests, such as a worker's own, which gives its git
/// credential's rewriting to what runs in a workspace, so only the code under test adds that.
fn machine(dir: &Path, config: &str) -> Environment {
    let global = dir.join("person.gitconfig");
    std::fs::write(&global, config).unwrap();
    given().with("GIT_CONFIG_GLOBAL", global)
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

    assert_eq!(without, RepoUrlReply { url: None }, "another remote isn't origin");
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

    let read = repo_url(&given(), dir.path()).await;

    assert_eq!(read.map_err(|e| e.code), Err(ErrorCode::Worker));
}
