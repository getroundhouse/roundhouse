use super::super::*;
use crate::git::tests::{new_repo, run};
use crate::git::{prepare_workspace, restore_workspace, worktree_there};
use protocol::worker::RemoveWorkspace;
use std::time::{Duration, Instant};

/// Long enough to delete a few files on a busy machine.
const DELETE_TIMEOUT: Duration = Duration::from_secs(30);

/// Lets a held deleter go when it goes, so a test that fails leaves nothing waiting.
struct Release(pub(super) PathBuf);

impl Drop for Release {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, "");
    }
}

/// Deletes as the product's own command does, without needing its binary.
pub(super) fn deleter() -> Deleter {
    Deleter::new("/bin/rm", ["-rf", "--"])
}

pub(super) struct Archived {
    pub(super) dir: tempfile::TempDir,
    pub(super) repo: PathBuf,
    pub(super) path: PathBuf,
}

impl Archived {
    /// A repository with a workspace called alder, whose worktree has ignored files, as a build's output is.
    pub(super) async fn ready() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        let root = dir.path().join("ws");
        let prepared =
            prepare_workspace(&root, "repo", repo.to_str().unwrap(), "alder", &"alder".into(), None).await.unwrap();
        let path = PathBuf::from(prepared.path);
        std::fs::create_dir_all(path.join("ignored/deep")).unwrap();
        std::fs::write(path.join("ignored/deep/output"), "built\n").unwrap();
        Self { dir, repo, path }
    }

    pub(super) async fn remove(&self, force: bool) -> Result<(), ApiError> {
        self.remove_with(force, &deleter()).await
    }

    pub(super) async fn remove_with(&self, force: bool, deleter: &Deleter) -> Result<(), ApiError> {
        remove_workspace(&self.removing(&self.path, force, None), deleter).await
    }

    /// The request to remove the worktree at `path`, the workspace's own or, with `own`, another made for it.
    pub(super) fn removing(&self, path: &Path, force: bool, own: Option<&Path>) -> RemoveWorkspace {
        let text = |path: &Path| path.to_str().unwrap().to_string();
        RemoveWorkspace { repo_path: text(&self.repo), path: text(path), force, own: own.map(text) }
    }

    pub(super) async fn restore(&self) -> Result<(), ApiError> {
        restore_workspace(self.repo.to_str().unwrap(), self.path.to_str().unwrap(), &"alder".into()).await
    }

    pub(super) fn deleting(&self) -> PathBuf {
        self.path.with_file_name(DELETING)
    }

    /// Moves the worktree aside by hand, as archiving would just before git forgets it.
    pub(super) fn move_aside(&self) -> PathBuf {
        std::fs::create_dir_all(self.deleting()).unwrap();
        let moved = self.deleting().join("alder-1");
        std::fs::rename(&self.path, &moved).unwrap();
        moved
    }

    /// Where git keeps its record of the worktree.
    pub(super) fn record(&self) -> PathBuf {
        PathBuf::from(run(&self.path, &["rev-parse", "--absolute-git-dir"]))
    }

    /// What's in [`DELETING`] waiting to be deleted.
    pub(super) fn waiting(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(self.deleting()) else { return Vec::new() };
        entries.map(|e| e.unwrap()).filter(|e| e.file_name() != LOCK).map(|e| e.path()).collect()
    }

    /// The worktrees git knows of, the repository's own included.
    pub(super) fn worktrees(&self) -> usize {
        run(&self.repo, &["worktree", "list", "--porcelain"]).lines().filter(|l| l.starts_with("worktree ")).count()
    }

    pub(super) fn has_branch(&self) -> bool {
        !run(&self.repo, &["branch", "--list", "alder"]).is_empty()
    }

    /// Waits for what was moved aside to be deleted, so nothing is still deleting it once the test ends.
    pub(super) async fn wait_until_deleted(&self) {
        assert!(self.deleting().is_dir(), "it was moved aside");
        let deadline = Instant::now() + DELETE_TIMEOUT;
        while !self.waiting().is_empty() {
            assert!(Instant::now() < deadline, "{:?} are still waiting to be deleted", self.waiting());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[tokio::test]
async fn archiving_frees_the_path_at_once_and_deletes_the_files_after() {
    let archived = Archived::ready().await;

    // Not forced, so the ignored files don't stand in its way.
    archived.remove(false).await.unwrap();

    assert!(!archived.path.exists());
    assert_eq!(archived.worktrees(), 1, "git has forgotten it");
    assert!(archived.has_branch());
    // Before its files are gone, which takes nothing from the restored worktree.
    archived.restore().await.unwrap();
    archived.wait_until_deleted().await;
    assert!(archived.path.join("a.txt").exists());
}

#[test]
fn archiving_returns_before_the_files_are_deleted_and_they_go_after_the_worker_has() {
    let worker = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let archived = worker.block_on(Archived::ready());
    let released = archived.dir.path().join("released");
    // It gives up once the test's directory has gone too, which a failed test deletes as it lets the deleter go.
    let script = "until [ -f \"$0\" ] || [ ! -d \"${0%/*}\" ]; do sleep 0.01; done; exec /bin/rm -rf -- \"$@\"";
    let held = Deleter::new("/bin/sh", ["-c", script, released.to_str().unwrap()]);
    let release = Release(released);

    let returned =
        worker.block_on(async { tokio::time::timeout(DELETE_TIMEOUT, archived.remove_with(false, &held)).await });

    returned.expect("it waited for the files to be deleted").unwrap();
    assert!(!archived.path.exists());
    assert_eq!(archived.waiting().len(), 1, "its files are still waiting");
    // Its tasks go with it, the one waiting on the deleter included.
    drop(worker);
    drop(release);
    tokio::runtime::Runtime::new().unwrap().block_on(archived.wait_until_deleted());
}

#[tokio::test]
async fn what_an_earlier_archive_left_is_deleted_with_the_next() {
    let archived = Archived::ready().await;
    let left = archived.deleting().join("birch-1");
    std::fs::create_dir_all(left.join("ignored")).unwrap();
    std::fs::write(left.join("ignored/output"), "built\n").unwrap();

    archived.remove(false).await.unwrap();

    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn a_worktree_moved_aside_by_a_worker_that_stopped_is_deleted_by_the_next_try() {
    let archived = Archived::ready().await;
    archived.move_aside();

    archived.remove(false).await.unwrap();

    assert_eq!(archived.worktrees(), 1, "git has forgotten it");
    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn a_worktree_git_forgot_while_it_was_aside_is_deleted_rather_than_put_back() {
    let archived = Archived::ready().await;
    let record = archived.record();
    let moved = archived.move_aside();
    // As restoring a workspace beside it would, or `git gc`.
    run(&archived.repo, &["worktree", "prune"]);

    forget(&archived.repo, &archived.path, Force::No, &record, &moved).await.unwrap();

    assert!(!archived.path.exists(), "a worktree git has forgotten would be broken");
    assert_eq!(archived.waiting(), [moved]);
}

#[tokio::test]
async fn a_refused_worktree_that_cant_go_back_is_kept_where_nothing_deletes_it() {
    let archived = Archived::ready().await;
    run(&archived.repo, &["worktree", "lock", archived.path.to_str().unwrap()]);
    let record = archived.record();
    let moved = archived.move_aside();
    // Such as a build writing to its absolute path.
    std::fs::create_dir_all(archived.path.join("target")).unwrap();

    let refused = forget(&archived.repo, &archived.path, Force::No, &record, &moved).await.err().unwrap();

    let parent = archived.path.parent().unwrap();
    let kept: Vec<PathBuf> = std::fs::read_dir(parent)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(".alder-kept-"))
        .collect();
    assert_eq!(kept.len(), 1, "{:?}", std::fs::read_dir(parent).unwrap().collect::<Vec<_>>());
    assert!(kept[0].join("ignored/deep/output").exists());
    assert!(refused.message.contains(&kept[0].display().to_string()), "{}", refused.message);
    assert!(archived.waiting().is_empty());
}

#[tokio::test]
async fn nothing_is_moved_aside_while_another_archive_holds_the_lock() {
    let archived = Archived::ready().await;
    std::fs::create_dir_all(archived.deleting()).unwrap();
    let other = File::create(archived.deleting().join(LOCK)).unwrap();
    other.lock().unwrap();
    let removing = archived.removing(&archived.path, false, None);

    let mut archiving = tokio::spawn(async move { remove_workspace(&removing, &deleter()).await });

    // Something that mustn't happen has no condition to wait on, so this is a bound. Right code never gets past it.
    let early = tokio::time::timeout(Duration::from_millis(500), &mut archiving).await;
    assert!(early.is_err(), "it went ahead while another archive held the lock");
    assert!(archived.path.join("a.txt").exists());
    drop(other);
    archiving.await.unwrap().unwrap();
    assert!(!archived.path.exists());
    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn a_locked_worktree_is_refused_and_left_where_it_was() {
    let archived = Archived::ready().await;
    run(&archived.repo, &["worktree", "lock", archived.path.to_str().unwrap()]);

    let refused = archived.remove(false).await.err().unwrap();
    // Git asks for a second --force, which only one it was stopped part way through making is given.
    let forced = archived.remove(true).await.err().unwrap();

    assert_eq!((refused.code, forced.code), (ErrorCode::Worker, ErrorCode::Worker));
    assert!(archived.path.join("ignored/deep/output").exists());
    assert_eq!(archived.worktrees(), 2);
    assert!(archived.waiting().is_empty());
}

#[tokio::test]
async fn a_worktree_holding_another_repository_is_refused_unless_forced() {
    let archived = Archived::ready().await;
    let nested = archived.path.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    run(&nested, &["init", "-q", "-b", "main"]);
    std::fs::write(nested.join("only-here.txt"), "work\n").unwrap();
    run(&nested, &["add", "."]);
    run(&nested, &["commit", "-qm", "a commit nowhere else"]);
    run(&archived.path, &["add", "nested"]);
    run(&archived.path, &["commit", "-qm", "the repository inside"]);
    assert_eq!(status(archived.path.to_str().unwrap()).await.unwrap().loss_if_removed(), None);

    let refused = archived.remove(false).await.err().unwrap();

    assert_eq!(refused.code, ErrorCode::Worker, "{}", refused.message);
    assert!(nested.join("only-here.txt").exists());
    archived.remove(true).await.unwrap();
    assert!(!archived.path.exists());
    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn a_worktree_that_cant_be_moved_aside_is_deleted_in_place() {
    let archived = Archived::ready().await;
    std::fs::write(archived.deleting(), "in the way\n").unwrap();

    archived.remove(false).await.unwrap();

    assert!(!archived.path.exists());
    assert_eq!(archived.worktrees(), 1);
}

#[tokio::test]
async fn uncommitted_work_is_refused_unless_forced_and_the_branch_is_kept() {
    let archived = Archived::ready().await;
    std::fs::write(archived.path.join("new.txt"), "work\n").unwrap();

    let refused = archived.remove(false).await.err().unwrap();

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    assert!(archived.path.join("new.txt").exists(), "a refused archive must not touch files");
    archived.remove(true).await.unwrap();
    assert!(!archived.path.exists());
    assert!(archived.has_branch());
    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn untracked_work_is_protected_even_if_git_is_set_to_hide_it() {
    let archived = Archived::ready().await;
    run(&archived.repo, &["config", "status.showUntrackedFiles", "no"]);
    std::fs::write(archived.path.join("agent-output.txt"), "precious\n").unwrap();

    let refused = archived.remove(false).await.err().unwrap();

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    assert!(archived.path.join("agent-output.txt").exists());
}

#[tokio::test]
async fn commits_only_reachable_from_a_detached_head_are_protected() {
    let archived = Archived::ready().await;
    run(&archived.path, &["checkout", "-q", "--detach"]);
    std::fs::write(archived.path.join("work.txt"), "work\n").unwrap();
    run(&archived.path, &["add", "work.txt"]);
    run(&archived.path, &["commit", "-qm", "on no branch"]);

    let refused = archived.remove(false).await.err().unwrap();

    assert_eq!(refused.code, ErrorCode::NeedsForce, "{}", refused.message);
    assert!(archived.path.join("work.txt").exists());
}

#[tokio::test]
async fn a_detached_head_on_a_commit_a_branch_holds_can_be_archived() {
    let archived = Archived::ready().await;
    run(&archived.path, &["checkout", "-q", "--detach"]);

    archived.remove(false).await.unwrap();

    assert!(!archived.path.exists());
    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn restoring_brings_back_committed_work_once() {
    let archived = Archived::ready().await;
    std::fs::write(archived.path.join("kept.txt"), "kept\n").unwrap();
    run(&archived.path, &["add", "kept.txt"]);
    run(&archived.path, &["commit", "-qm", "work"]);
    archived.remove(false).await.unwrap();

    archived.restore().await.unwrap();

    assert_eq!(std::fs::read_to_string(archived.path.join("kept.txt")).unwrap(), "kept\n");
    assert_eq!(archived.restore().await.err().unwrap().code, ErrorCode::Conflict);
    archived.wait_until_deleted().await;
}

/// What a hub asks of a workspace an earlier one left part way through being archived or restored.
#[tokio::test]
async fn a_worktree_is_there_only_while_git_reads_one_at_its_path() {
    let archived = Archived::ready().await;
    let path = archived.path.to_str().unwrap();
    assert!(worktree_there(path).await.unwrap());
    // A folder inside it is in a worktree, and isn't one.
    assert!(!worktree_there(archived.path.join("ignored").to_str().unwrap()).await.unwrap());

    archived.remove(true).await.unwrap();
    assert!(!worktree_there(path).await.unwrap());
    // Nor is a folder left where it was, which restoring refuses to write over.
    std::fs::create_dir_all(archived.path.join("left")).unwrap();
    assert!(!worktree_there(path).await.unwrap());
    std::fs::remove_dir_all(&archived.path).unwrap();

    archived.restore().await.unwrap();
    assert!(worktree_there(path).await.unwrap());
    archived.wait_until_deleted().await;
}

#[tokio::test]
async fn a_worktree_deleted_by_hand_can_still_be_archived_and_restored() {
    let archived = Archived::ready().await;
    std::fs::remove_dir_all(&archived.path).unwrap();

    archived.remove(false).await.unwrap();

    assert_eq!(archived.worktrees(), 1);
    archived.restore().await.unwrap();
    assert!(archived.path.join("a.txt").exists());
}

/// A `git worktree add` stopped part way leaves git's own lock on what it made, which nothing takes off, and a forced
/// removal goes past, as it doesn't a lock a person put there.
#[tokio::test]
async fn one_git_was_stopped_part_way_through_making_is_removed_when_forced() {
    let archived = Archived::ready().await;
    run(&archived.repo, &["worktree", "lock", "--reason", "initializing", archived.path.to_str().unwrap()]);

    archived.remove(true).await.unwrap();

    assert!(!archived.path.exists());
    assert!(!run(&archived.repo, &["worktree", "list"]).contains("alder"), "git forgot it");
}
