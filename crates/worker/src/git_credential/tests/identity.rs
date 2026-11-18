//! Who commits are by under the files, with real git, on a machine of the test's own, whose git config says who its
//! own commits are by.

use std::path::{Path, PathBuf};
use std::process::Command;

use process::Environment;
use process::testing::WholeEnvironment;
use protocol::model::BranchName;
use protocol::worker::RepoSource;
use tempfile::TempDir;

use super::super::files::{Files, Handed};
use super::{ada, files, installation};
use crate::git::tests::new_repo;
use crate::git::{Clones, prepare_workspace};
use crate::testing::given;

/// Who the machine's own git config says commits are by.
const MACHINES_OWN: &str = "Ada Lovelace <ada@example.com>";
/// Who the installation's commits are by.
const BOT: &str = "ada-roundhouse[bot] <41+ada-roundhouse[bot]@users.noreply.github.com>";
/// A repository on github.com, by its address over HTTPS.
const ON_GITHUB: &str = "https://github.com/ada/rails.git";

/// A machine with a home of its own, whose git config says who commits are by, and the worker's files, under an app's
/// installation's credential, which has its bot as its commit identity.
struct Machine {
    home: TempDir,
    state: TempDir,
    files: Files,
}

impl Machine {
    fn new() -> Self {
        let (home, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let config = "[user]\n\tname = Ada Lovelace\n\temail = ada@example.com\n";
        std::fs::write(home.path().join(".gitconfig"), config).unwrap();
        let files = files(state.path());
        files.write(&Handed::Account(installation())).unwrap();
        Self { home, state, files }
    }

    /// `git` in `dir`, given the files, as what the worker starts is, with the machine's own config but for the
    /// system's, which can say anything, as "What still goes round it" says.
    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let mut environment = WholeEnvironment::for_a_test(self.home.path());
        environment.set("GIT_CONFIG_NOSYSTEM", "1");
        let mut git = Command::new("git");
        git.args(args).current_dir(dir);
        self.files.environment(&environment.given()).apply(&mut git);
        let output = git.output().unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    /// A repository at `dir` whose `origin` is at `address`, or that has no remote.
    fn repo(&self, dir: &Path, origin: Option<&str>) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        self.git(dir, &["init", "--quiet"]);
        if let Some(address) = origin {
            self.git(dir, &["remote", "add", "origin", address]);
        }
        dir.to_path_buf()
    }

    /// Who the commit git makes in `repo` now is by, and who committed it.
    fn commit(&self, repo: &Path) -> String {
        self.git(repo, &["commit", "--allow-empty", "--quiet", "-m", "made up"]);
        self.git(repo, &["log", "-1", "--format=%an <%ae>, %cn <%ce>"])
    }

    fn identity(&self) -> PathBuf {
        self.state.path().join("git-credential").join("identity")
    }
}

/// As `git log` says a commit is by `who`, who committed it too.
fn by(who: &str) -> String {
    format!("{who}, {who}\n")
}

/// Over the machine's own, whichever of GitHub's addresses `origin` has, as the helper is given for each.
#[test]
fn a_repository_on_github_commits_as_the_credentials_identity_whatever_form_its_address_takes() {
    let (machine, dir) = (Machine::new(), tempfile::tempdir().unwrap());
    let addresses = [
        ON_GITHUB,
        "https://ada@github.com/ada/rails.git",
        "git@github.com:ada/rails.git",
        "ssh://git@github.com/ada/rails.git",
    ];

    for (i, address) in addresses.into_iter().enumerate() {
        let repo = machine.repo(&dir.path().join(i.to_string()), Some(address));
        assert_eq!(machine.commit(&repo), by(BOT), "{address}");
    }
}

/// As on GitHub Enterprise, on GitLab, at an address that only starts as GitHub's does, or with no remote at all, which
/// the helper isn't for.
#[test]
fn a_repository_elsewhere_or_with_no_remote_commits_as_the_machines_own() {
    let (machine, dir) = (Machine::new(), tempfile::tempdir().unwrap());
    let elsewhere = [
        Some("https://github.example.com/ada/rails.git"),
        Some("git@gitlab.com:ada/rails.git"),
        Some("https://github.com.example.com/ada/rails.git"),
        None,
    ];

    for (i, origin) in elsewhere.into_iter().enumerate() {
        let repo = machine.repo(&dir.path().join(i.to_string()), origin);
        assert_eq!(machine.commit(&repo), by(MACHINES_OWN), "{origin:?}");
    }
}

/// Workspaces' worktrees as the worker makes them: of a checkout whose `origin` is on github.com, which its worktrees
/// share, and of the worker's own clone of a repository's address on github.com, which is the clone's `origin`. Each
/// git the worker runs here is told the address is the checkout, since nothing reaches GitHub from a test.
#[tokio::test]
async fn a_workspaces_worktree_commits_as_the_credentials_identity_from_a_checkout_or_a_clone() {
    let (machine, dir) = (Machine::new(), tempfile::tempdir().unwrap());
    let checkout = new_repo(dir.path());
    machine.git(&checkout, &["remote", "add", "origin", ON_GITHUB]);
    let reaching = given()
        .with("GIT_CONFIG_COUNT", "1")
        .with("GIT_CONFIG_KEY_0", format!("url.{}.insteadOf", checkout.display()))
        .with("GIT_CONFIG_VALUE_0", ON_GITHUB);
    let clones = Clones::new(dir.path().join("clones"));
    let clone = clones.made_for(&reaching, &RepoSource::Clone { url: ON_GITHUB.into() }).await.unwrap();
    let root = dir.path().join("workspaces");

    let of_the_checkout = worktree(&reaching, &root, &checkout, "alder").await;
    let of_the_clone = worktree(&reaching, &root, &clone, "birch").await;

    assert_eq!(machine.commit(&of_the_checkout), by(BOT), "a worktree of the checkout");
    assert_eq!(machine.commit(&of_the_clone), by(BOT), "a worktree of the clone");
}

/// The worktree of the workspace `name`, made of `repo` in `root`, as the worker makes it, given `given`.
async fn worktree(given: &Environment, root: &Path, repo: &Path, name: &str) -> PathBuf {
    let branch = BranchName::from(format!("{name}/start"));
    let ready = prepare_workspace(given, root, "rails", repo, name, &branch, None).await.unwrap();
    PathBuf::from(ready.path)
}

/// Over the repository's own, as the person's would be, and only while the credential says so: one without a commit
/// identity, and a block, take it out again.
#[test]
fn commits_are_by_the_credentials_identity_while_it_has_one() {
    let (machine, dir) = (Machine::new(), tempfile::tempdir().unwrap());
    let repo = machine.repo(dir.path(), Some(ON_GITHUB));
    machine.git(&repo, &["config", "user.name", "Grace Hopper"]);
    machine.git(&repo, &["config", "user.email", "grace@example.com"]);

    let as_the_bot = machine.commit(&repo);
    machine.files.write(&Handed::Account(ada())).unwrap();
    let as_ada = (machine.commit(&repo), machine.identity().exists());
    machine.files.write(&Handed::Account(installation())).unwrap();
    machine.files.write(&Handed::Blocked("Renew ada in Settings".into())).unwrap();
    let blocked = (machine.commit(&repo), machine.identity().exists());

    assert_eq!(as_the_bot, by(BOT));
    let own = by("Grace Hopper <grace@example.com>");
    assert_eq!((as_ada, blocked), ((own.clone(), false), (own, false)));
}
