use super::locking::{Changing, Hold, WAITING, Worktrees};
use crate::git::tests::{new_repo, prepare, run};
use process::Environment;
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs::{OpenOptions, TryLockError};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;
use std::time::Duration;

/// Long enough for git on a busy machine.
const TIMEOUT: Duration = Duration::from_secs(10);

/// A worktree of a repository part way through being made, as `git worktree add` leaves its entry for a moment: git's
/// own lock and its `gitdir` written, and its `commondir` made but not yet written. Git 2.44 and later write `HEAD`
/// only after `commondir`, and git before has a placeholder `HEAD` written by then, as this does. The commands that
/// fail on one fail either way. With the repository's worktrees held alone, as a worker making one holds them.
struct BeingMade {
    entry: PathBuf,
    _changing: Changing,
}

impl BeingMade {
    async fn in_repo(repo: &Path) -> Self {
        let changing = Worktrees::of(&Environment::NONE, repo).await.unwrap().changing().await.unwrap();
        let common = run(repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
        let entry = Path::new(&common).join("worktrees").join("being-made");
        std::fs::create_dir_all(&entry).unwrap();
        let worktree = repo.with_file_name("being-made");
        std::fs::write(entry.join("gitdir"), format!("{}\n", worktree.join(".git").display())).unwrap();
        std::fs::write(entry.join("locked"), "initializing\n").unwrap();
        std::fs::write(entry.join("HEAD"), format!("{}\n", "0".repeat(40))).unwrap();
        std::fs::write(entry.join("commondir"), "").unwrap();
        Self { entry, _changing: changing }
    }

    /// Done with, as when git gives up on it, and the lock let go of.
    fn done(self) {
        std::fs::remove_dir_all(&self.entry).unwrap();
    }
}

/// Runs `work` while a worktree of `repo` is being made, and has it done once `work` waits for the lock, as a worker
/// does. So `work` fails if it runs a git command that the half-made entry breaks without waiting for the lock first.
/// Panics if `work` ends before it has waited.
///
/// Not an `async fn`, whose future would hold `work` whole: making a workspace is a future of about 16 KB, so one that
/// held it would be past clippy's `large_futures` on Linux. This boxes `work`, and its future holds only the box.
pub(crate) fn while_one_is_made<T: Debug>(repo: &Path, work: impl Future<Output = T>) -> impl Future<Output = T> {
    let mut work = Box::pin(work);
    async move {
        let being_made = BeingMade::in_repo(repo).await;
        // Only once it's being made, so a wait of its own for the lock isn't taken for the work's.
        let logs = Logs::capture();
        tokio::select! {
            biased;
            () = logs.until(WAITING) => {}
            ended = &mut work => panic!("it never waited for the worktree being made, and ended with {ended:?}"),
        }
        being_made.done();
        tokio::time::timeout(TIMEOUT, work).await.expect("it never finished once the worktree was made")
    }
}

type Buffer = Rc<RefCell<Vec<u8>>>;

thread_local! {
    static CAPTURED: RefCell<Option<Buffer>> = const { RefCell::new(None) };
}

/// What's logged on this thread, as it's logged, for as long as this is held. A `#[tokio::test]` runs everything it
/// awaits on its own thread.
pub(crate) struct Logs {
    written: Buffer,
}

struct Writing(Option<Buffer>);

impl Write for Writing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(written) = &self.0 {
            written.borrow_mut().extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Logs {
    pub(crate) fn capture() -> Self {
        // With a thread-local subscriber, another test can first reach a callsite without one and cache that
        // nobody wants it. Keep one global subscriber, and route its writes to the capture on this thread only.
        static SUBSCRIBER: Once = Once::new();
        SUBSCRIBER.call_once(|| {
            let subscriber = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(|| Writing(CAPTURED.with(|written| written.borrow().clone())))
                .finish();
            tracing::subscriber::set_global_default(subscriber).unwrap();
        });
        let written = Rc::new(RefCell::new(Vec::new()));
        CAPTURED.with(|captured| assert!(captured.replace(Some(written.clone())).is_none(), "already capturing"));
        Self { written }
    }

    pub(crate) async fn until(&self, line: &str) {
        waiting::task::until(TIMEOUT, format!("nothing logged {line:?}"), || {
            String::from_utf8_lossy(&self.written.borrow()).contains(line)
        })
        .await;
    }
}

impl Drop for Logs {
    fn drop(&mut self) {
        CAPTURED.with(RefCell::take);
    }
}

/// Whether the lock's file could be held now, as `how` says, by something other than this test.
fn free(worktrees: &Worktrees, how: Hold) -> bool {
    let file = OpenOptions::new().read(true).open(&worktrees.lock).unwrap();
    let tried = match how {
        Hold::Alone => file.try_lock(),
        Hold::BesideOthersReading => file.try_lock_shared(),
    };
    match tried {
        Ok(()) => true,
        Err(TryLockError::WouldBlock) => false,
        Err(TryLockError::Error(e)) => panic!("{e}"),
    }
}

/// Waits for nothing to hold the lock's file. A process that another test in this process starts while this one holds
/// it has a copy of the file until it starts its program, which holds the lock a millisecond or so longer. Under
/// nextest, which runs each test in a process of its own, none does.
fn comes_free(worktrees: &Worktrees) {
    waiting::thread::until(TIMEOUT, "the lock was kept after it went", || free(worktrees, Hold::Alone));
}

#[tokio::test]
async fn changing_holds_the_lock_alone_and_reading_beside_others_reading() {
    let dir = tempfile::tempdir().unwrap();
    let worktrees = Worktrees::of(&Environment::NONE, &new_repo(dir.path())).await.unwrap();

    let changing = worktrees.changing().await.unwrap();
    assert!(!free(&worktrees, Hold::BesideOthersReading), "something read the worktrees while they were being changed");
    drop(changing);

    let reading = worktrees.reading().await.unwrap();
    assert!(free(&worktrees, Hold::BesideOthersReading), "only one could read them at a time");
    assert!(!free(&worktrees, Hold::Alone), "something changed the worktrees while they were being read");
    drop(reading);
    comes_free(&worktrees);
}

/// As in a repository shared with someone whose worker made the lock's file. Run as root, which can write it anyway,
/// this can't fail.
#[tokio::test]
async fn a_lock_this_user_cant_write_is_still_held() {
    let dir = tempfile::tempdir().unwrap();
    let worktrees = Worktrees::of(&Environment::NONE, &new_repo(dir.path())).await.unwrap();
    drop(worktrees.changing().await.unwrap());
    let mut read_only = std::fs::metadata(&worktrees.lock).unwrap().permissions();
    read_only.set_readonly(true);
    std::fs::set_permissions(&worktrees.lock, read_only).unwrap();

    let changing = worktrees.changing().await.unwrap();

    assert!(!free(&worktrees, Hold::BesideOthersReading), "something read the worktrees while they were being changed");
    drop(changing);
}

#[tokio::test]
async fn a_repository_and_its_worktrees_share_one_lock_and_another_repository_has_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let workspace = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let other = tempfile::tempdir().unwrap();

    let ours = Worktrees::of(&Environment::NONE, &repo).await.unwrap().lock;
    assert_eq!(Worktrees::of(&Environment::NONE, Path::new(&workspace.path)).await.unwrap().lock, ours);
    assert_ne!(Worktrees::of(&Environment::NONE, &new_repo(other.path())).await.unwrap().lock, ours);
}

/// A callsite can first be reached on another test's thread, which has no thread-local subscriber.
#[tokio::test]
async fn logs_are_captured_even_when_another_thread_logs_first() {
    fn log() {
        tracing::info!("a captured worktree log");
    }
    let logs = Logs::capture();

    std::thread::spawn(log).join().unwrap();
    assert!(logs.written.borrow().is_empty(), "captured another thread's log");
    log();

    logs.until("a captured worktree log").await;
}
