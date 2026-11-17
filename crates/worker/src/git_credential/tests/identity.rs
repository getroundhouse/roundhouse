use std::path::Path;
use std::process::Command;

use protocol::model::GithubToken;
use protocol::worker::{GitCredential, GitIdentity};

use super::super::files::{Files, Handed};
use super::{ada, files};

/// An app's installation, whose commits are by its bot.
fn installation() -> GitCredential {
    GitCredential {
        username: "x-access-token".into(),
        token: GithubToken::new("made-up"),
        identity: Some(GitIdentity {
            name: "ada-roundhouse[bot]".to_string(),
            email: "41+ada-roundhouse[bot]@users.noreply.github.com".to_string(),
        }),
    }
}

/// `git` in `repo`, given `files`, without the person's own config, as "What still goes round it" says.
fn git(repo: &Path, files: &Files, args: &[&str]) -> String {
    let mut git = Command::new("git");
    git.args(args).current_dir(repo).env("HOME", repo).env("GIT_CONFIG_NOSYSTEM", "1").env_remove("XDG_CONFIG_HOME");
    files.environment().apply(&mut git);
    let output = git.output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// Who the commit `git` makes in `repo` now is by, and who committed it.
fn commit(repo: &Path, files: &Files) -> String {
    git(repo, files, &["commit", "--allow-empty", "--quiet", "-m", "made up"]);
    git(repo, files, &["log", "-1", "--format=%an <%ae>, %cn <%ce>"])
}

/// Over the repository's own, as the person's would be, and only while the credential says so: one without an
/// identity, and a block, take it out again.
#[test]
fn commits_are_by_the_credentials_identity_while_it_has_one() {
    let (state, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let files = files(state.path());
    files.write(&Handed::Blocked("Renew ada in Settings".into())).unwrap();
    git(repo.path(), &files, &["init", "--quiet"]);
    git(repo.path(), &files, &["config", "user.name", "Ada Lovelace"]);
    git(repo.path(), &files, &["config", "user.email", "ada@example.com"]);

    files.write(&Handed::Account(installation())).unwrap();
    let as_the_bot = commit(repo.path(), &files);
    files.write(&Handed::Account(ada())).unwrap();
    let as_ada = commit(repo.path(), &files);
    files.write(&Handed::Account(installation())).unwrap();
    files.write(&Handed::Blocked("Renew ada in Settings".into())).unwrap();
    let blocked = commit(repo.path(), &files);

    let bot = "ada-roundhouse[bot] <41+ada-roundhouse[bot]@users.noreply.github.com>";
    assert_eq!(as_the_bot, format!("{bot}, {bot}\n"));
    let own = "Ada Lovelace <ada@example.com>";
    assert_eq!((as_ada, blocked), (format!("{own}, {own}\n"), format!("{own}, {own}\n")));
}
