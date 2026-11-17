use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use process::Environment;
use protocol::client::ErrorCode;
use protocol::worker::RepoSource;

use super::Clones;
use super::making::WAITING;
use super::place::MAKING;
use crate::git::tests::{new_repo, prepare, run};
use crate::git::worktrees::tests::Logs;
use crate::testing::given;

/// Long enough for git on a busy machine.
const TIMEOUT: Duration = Duration::from_secs(10);

/// An address nothing answers at, which git fails to reach at once, since the name is reserved never to be one.
const UNREACHABLE: &str = "https://roundhouse.invalid/ada/rails.git";

/// A repository's remote, a bare repository with `main`, and the checkout that pushes to it.
struct Remote {
    checkout: PathBuf,
    bare: PathBuf,
}

impl Remote {
    fn new(dir: &Path) -> Self {
        let checkout = new_repo(dir);
        let bare = dir.join("remote.git");
        run(dir, &["clone", "-q", "--bare", checkout.to_str().unwrap(), bare.to_str().unwrap()]);
        Self { checkout, bare }
    }

    fn url(&self) -> String {
        self.bare.to_string_lossy().into_owned()
    }

    /// Commits on `main`, pushes it, and says the commit.
    fn push_another(&self) -> String {
        run(&self.checkout, &["commit", "-q", "--allow-empty", "-m", "more"]);
        run(&self.checkout, &["push", "-q", self.bare.to_str().unwrap(), "main"]);
        run(&self.checkout, &["rev-parse", "HEAD"])
    }

    fn main(&self) -> String {
        run(&self.bare, &["rev-parse", "main"])
    }
}

fn clone_of(url: &str) -> RepoSource {
    RepoSource::Clone { url: url.into() }
}

/// What the worker gives every git, as it gives the git credential its hub set, here configuration that has `url`
/// reach `remote`.
fn reaching(url: &str, remote: &Remote) -> Environment {
    given()
        .with("GIT_CONFIG_COUNT", "1")
        .with("GIT_CONFIG_KEY_0", format!("url.{}.insteadOf", remote.url()))
        .with("GIT_CONFIG_VALUE_0", url)
}

/// The clones in `dir`, leaving out the folder they're made in.
fn listed(dir: &Path) -> Vec<String> {
    let names = std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap());
    names.filter(|name| name != MAKING).collect()
}

/// The first workspace of a repository given by its address is made through a clone, made then, and the next through
/// the same clone, fetched first, so it starts from what the remote has now. The clone is bare, and has no branch but
/// the workspaces'.
#[tokio::test]
async fn a_clone_is_made_once_and_fetched_before_each_workspace_made_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let remote = Remote::new(dir.path());
    let (clones, root) = (Clones::new(dir.path().join("clones")), dir.path().join("ws"));
    let source = clone_of(&remote.url());

    let first_from = clones.made_for(&given(), &source).await.unwrap();
    let first = prepare(&root, &first_from, "alder", None).await.unwrap();
    let pushed = remote.push_another();
    let second_from = clones.made_for(&given(), &source).await.unwrap();
    let second = prepare(&root, &second_from, "birch", None).await.unwrap();

    assert_eq!(first_from, second_from);
    assert_eq!(listed(&dir.path().join("clones")), [first_from.file_name().unwrap().to_str().unwrap()]);
    assert_eq!((first.base_ref.as_str(), &first.warnings), ("origin/main", &Vec::<String>::new()));
    assert_ne!(run(Path::new(&first.path), &["rev-parse", "HEAD"]), pushed);
    assert_eq!(run(Path::new(&second.path), &["rev-parse", "HEAD"]), pushed, "fetched before the second was made");
    assert_eq!(run(&first_from, &["rev-parse", "--is-bare-repository"]), "true");
    let branches = run(&first_from, &["for-each-ref", "--format=%(refname:short)", "refs/heads"]);
    assert_eq!(branches, "alder/start\nbirch/start");
}

/// A worker that wants a clone another is making waits its turn, and then takes the one the other made, rather than
/// making one beside it or over it. The test holds the turn, as the other worker, and makes the clone while the worker
/// waits, of an address the worker couldn't clone itself.
#[tokio::test]
async fn a_worker_waits_for_another_making_the_clone_and_takes_the_one_it_made() {
    let dir = tempfile::tempdir().unwrap();
    let remote = Remote::new(dir.path());
    let clones = Clones::new(dir.path().join("clones"));
    let (making, lock) = clones.making(UNREACHABLE);
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let other = File::create(&lock).unwrap();
    other.lock().unwrap();
    let (given, source) = (given(), clone_of(UNREACHABLE));
    let logs = Logs::capture();
    let wanting = clones.made_for(&given, &source);
    tokio::pin!(wanting);

    tokio::select! {
        biased;
        () = logs.until(WAITING) => {}
        ended = &mut wanting => panic!("it didn't wait its turn, and ended with {ended:?}"),
    }
    run(dir.path(), &["clone", "-q", "--bare", &remote.url(), clones.of(UNREACHABLE).to_str().unwrap()]);
    drop(other);
    let taken = tokio::time::timeout(TIMEOUT, wanting).await.expect("it never took its turn").unwrap();

    assert_eq!(taken, clones.of(UNREACHABLE));
    assert!(!making.exists(), "it made one of its own");
}

/// A clone that a stop cut short, as a worker killed while it fetched leaves one, isn't taken for whole: the next worker
/// to want it makes it again. The remote is a server on 127.0.0.1 that never answers, so the clone stops part way, and
/// then the address is pointed at a repository, through what git is given.
#[tokio::test]
async fn a_clone_cut_short_is_made_again_rather_than_taken_for_whole() {
    let dir = tempfile::tempdir().unwrap();
    let remote = Remote::new(dir.path());
    let clones = Arc::new(Clones::new(dir.path().join("clones")));
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/ada/rails.git", silent.local_addr().unwrap());
    let (cloning, cut_url) = (clones.clone(), url.clone());
    let cut_short = tokio::spawn(async move {
        let given = given();
        cloning.made_for(&given, &clone_of(&cut_url)).await
    });
    let accepted = tokio::time::timeout(TIMEOUT, silent.accept()).await.expect("git never asked the remote");

    cut_short.abort();
    assert!(cut_short.await.unwrap_err().is_cancelled());
    // So a git that reaches for the server again is refused at once, rather than left waiting for an answer.
    drop((accepted, silent));
    let made = clones.made_for(&reaching(&url, &remote), &clone_of(&url)).await.unwrap();

    assert_eq!(run(&made, &["rev-parse", "refs/remotes/origin/main"]), remote.main());
    assert_eq!(run(&made, &["symbolic-ref", "refs/remotes/origin/HEAD"]), "refs/remotes/origin/main");
}

/// Every git a clone runs is given what the worker gives every git it runs, which is how the git credential its hub
/// set reaches the clone's.
#[tokio::test]
async fn a_clone_s_git_is_given_what_the_worker_gives_every_git() {
    let dir = tempfile::tempdir().unwrap();
    let remote = Remote::new(dir.path());
    let clones = Clones::new(dir.path().join("clones"));

    let made = clones.made_for(&reaching(UNREACHABLE, &remote), &clone_of(UNREACHABLE)).await.unwrap();

    assert_eq!(run(&made, &["rev-parse", "refs/remotes/origin/main"]), remote.main());
}

/// A checkout is where git runs, as before there were clones, and nothing is made for it in the clones directory. An
/// address git would take for an option is refused before git is run.
#[tokio::test]
async fn a_checkout_is_used_as_it_is_and_an_address_like_an_option_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let clones = Clones::new(dir.path().join("clones"));
    let checkout = RepoSource::Checkout { path: repo.to_string_lossy().into_owned() };

    let made = clones.made_for(&given(), &checkout).await.unwrap();
    let refused = clones.made_for(&given(), &clone_of("--upload-pack=touch /tmp/x")).await.unwrap_err();

    assert_eq!((&made, &clones.repository(&checkout)), (&repo, &repo));
    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    assert!(!dir.path().join("clones").exists());
}
