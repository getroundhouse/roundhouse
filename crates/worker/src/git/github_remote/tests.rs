use process::Environment;
use protocol::model::GithubRepositoryName;

use super::address::on_github;
use super::github_remote;
use crate::git::tests::{new_repo, run};

const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

/// Every way git writes an address of github.com, with or without `.git`, a `/` at its end, a user, a password or a
/// port, whatever the host's case.
#[test]
fn an_address_of_github_names_its_owner_and_name() {
    for address in [
        "https://github.com/cli/cli.git",
        "https://github.com/cli/cli",
        "https://github.com/cli/cli/",
        "https://ada:a-token@github.com/cli/cli.git",
        "HTTPS://GitHub.com/cli/cli.git",
        "http://github.com/cli/cli",
        "ssh://git@github.com/cli/cli.git",
        "ssh://git@github.com:22/cli/cli.git",
        "git+ssh://git@github.com/cli/cli",
        "git://github.com/cli/cli.git",
        "git@github.com:cli/cli.git",
        "git@github.com:/cli/cli.git",
        "github.com:cli/cli",
    ] {
        assert_eq!(on_github(address), Some(CLI), "{address}");
    }
}

/// Another host, which the default isn't for, a path, and what has no owner and name.
#[test]
fn any_other_address_names_none() {
    for address in [
        "https://gitlab.com/cli/cli.git",
        "https://github.example.com/cli/cli.git",
        "https://notgithub.com/cli/cli.git",
        "git@gitlab.com:cli/cli.git",
        "/code/cli",
        "./cli:cli",
        "file:///code/cli.git",
        "https://github.com/cli",
        "https://github.com/cli/cli/pulls",
        "https://github.com/",
        "git@github.com:cli",
        "",
    ] {
        assert_eq!(on_github(address), None, "{address}");
    }
}

/// As git gives the address to push to, so the push address, and the person's own rewriting of addresses, count.
#[tokio::test]
async fn a_repository_pushes_where_git_says_its_origin_pushes() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let read = || async { github_remote(&Environment::NONE, &repo).await.unwrap() };

    let without = read().await;
    run(&repo, &["remote", "add", "origin", "/code/upstream"]);
    let elsewhere = read().await;
    run(&repo, &["remote", "set-url", "--push", "origin", "git@github.com:cli/cli.git"]);
    let pushes_to_github = read().await;
    run(&repo, &["config", "url.https://github.com/.insteadOf", "gh:"]);
    run(&repo, &["remote", "set-url", "--push", "origin", "gh:octo/hello"]);
    let rewritten = read().await;

    assert_eq!((without, elsewhere), (None, None));
    assert_eq!(pushes_to_github, Some(CLI));
    assert_eq!(rewritten, Some(GithubRepositoryName::named("octo/hello")));
}

#[tokio::test]
async fn a_directory_that_isnt_a_repository_is_an_error() {
    let dir = tempfile::tempdir().unwrap();

    let refused = github_remote(&Environment::NONE, dir.path()).await;

    assert!(refused.is_err(), "{refused:?}");
}
