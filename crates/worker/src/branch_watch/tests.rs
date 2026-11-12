use std::path::Path;

use protocol::model::{BranchName, Branches, Head};

use super::*;
use crate::git::tests::{new_repo, run};

/// Long enough for a busy machine, and only waited out when the watcher has failed to send.
const PATIENCE: Duration = Duration::from_secs(20);

/// How long a watcher has to stay asleep to count as not woken, which is waited out each time.
const QUIET: Duration = Duration::from_millis(500);

struct Watching {
    worktree: PathBuf,
    frames: mpsc::Receiver<WorkerFrame>,
    _stopping: watch::Sender<bool>,
}

impl Watching {
    /// Once it's said it's watching, by when it has sent the first frame.
    async fn start(worktree: &Path) -> Self {
        let (frames, received) = mpsc::channel(16);
        let stopping = watch::Sender::new(false);
        let (first, sent) = oneshot::channel();
        tokio::spawn(watch(worktree.to_path_buf(), frames, stopping.subscribe(), first));
        tokio::time::timeout(PATIENCE, sent).await.expect("it never said").unwrap();
        Self { worktree: worktree.to_path_buf(), frames: received, _stopping: stopping }
    }

    async fn next(&mut self) -> Branches {
        let frame = tokio::time::timeout(PATIENCE, self.frames.recv()).await.expect("no frame came").unwrap();
        let WorkerFrame::Branches { branches } = frame else { panic!("not the branches: {frame:?}") };
        branches
    }

    fn git(&self, args: &[&str]) {
        run(&self.worktree, args);
    }
}

/// The OS's watcher for `worktree`, once anything it heard of before it was watching has come and gone, and what it
/// wakes.
async fn listening(worktree: &Path) -> (Arc<Wakes>, Option<Box<dyn Watcher + Send>>) {
    let places = branches::places(worktree).await.unwrap();
    let read = branches::read(worktree, &places).await.unwrap();
    let wakes = Arc::new(Wakes { places, last: Mutex::new(Some(read)), wake: Notify::new() });
    let watcher = start(&wakes);
    let _ = tokio::time::timeout(QUIET, wakes.wake.notified()).await;
    (wakes, watcher)
}

async fn woke(wakes: &Wakes, within: Duration) -> bool {
    tokio::time::timeout(within, wakes.wake.notified()).await.is_ok()
}

fn heard(kind: EventKind, file: PathBuf) -> Event {
    Event::new(kind).add_path(file)
}

/// Every git command opens `HEAD` and the config, which inotify tells of, so a watcher woken by that would read again
/// after each of its own reads, for ever.
#[tokio::test]
async fn reading_the_branches_wakes_nothing_and_switching_does() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let worktree = dir.path().join("aspen");
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    let (wakes, _watcher) = listening(&worktree).await;

    branches::read(&worktree, &wakes.places).await.unwrap();
    let read = woke(&wakes, QUIET).await;
    run(&worktree, &["switch", "-q", "-c", "header"]);
    let switched = woke(&wakes, PATIENCE).await;

    assert!(!read, "woken by its own read");
    assert!(switched, "so it was listening");
}

/// Another worktree's commit rewrites only its own branch's ref, and its own files.
#[tokio::test]
async fn another_worktrees_commit_wakes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let (worktree, elm) = (dir.path().join("aspen"), dir.path().join("elm"));
    for (path, name) in [(&worktree, "aspen"), (&elm, "elm")] {
        run(&repo, &["worktree", "add", "-q", "--no-track", "-b", name, path.to_str().unwrap(), "main"]);
    }
    let (wakes, _watcher) = listening(&worktree).await;

    run(&elm, &["commit", "-q", "--allow-empty", "-m", "on elm"]);
    let elsewhere = woke(&wakes, QUIET).await;
    run(&worktree, &["commit", "-q", "--allow-empty", "-m", "on aspen"]);
    let here = woke(&wakes, PATIENCE).await;

    assert!(!elsewhere, "woken by another worktree's commit");
    assert!(here, "so it was listening");
}

/// As inotify tells of them, whatever the OS the test runs on.
#[tokio::test]
async fn a_file_only_opened_or_read_doesnt_matter_and_one_written_does() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let places = branches::places(&repo).await.unwrap();
    let wakes = Wakes { places, last: Mutex::new(None), wake: Notify::new() };
    let head = repo.canonicalize().unwrap().join(".git").join("HEAD");
    let access = |kind| heard(EventKind::Access(kind), head.clone());

    let opened = [AccessKind::Open(AccessMode::Any), AccessKind::Read, AccessKind::Close(AccessMode::Read)];
    let written = [
        heard(EventKind::Modify(notify::event::ModifyKind::Any), head.clone()),
        access(AccessKind::Close(AccessMode::Write)),
    ];

    assert!(!opened.into_iter().any(|kind| mattered(&wakes, &Ok(access(kind)))));
    assert!(written.into_iter().all(|event| mattered(&wakes, &Ok(event))));
}

#[tokio::test]
async fn it_sends_the_branches_at_once_and_again_when_they_change() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let worktree = dir.path().join("aspen");
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    let mut watching = Watching::start(&worktree).await;

    let at_once = watching.frames.try_recv().expect("sent before it said it was watching");
    let WorkerFrame::Branches { branches: at_once } = at_once else { panic!() };
    watching.git(&["switch", "-q", "-c", "header"]);
    let switched = watching.next().await;

    assert_eq!(at_once.head, Head::Branch { name: BranchName::from("aspen") });
    assert_eq!(switched.head, Head::Branch { name: BranchName::from("header") });
}

/// A push rewrites only the ref for the branch upstream, which it then has to be watching.
#[tokio::test]
async fn it_sends_a_push_of_a_branch_already_pushed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let origin = dir.path().join("origin.git");
    run(dir.path(), &["init", "-q", "--bare", origin.to_str().unwrap()]);
    run(&repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
    let worktree = dir.path().join("aspen");
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    run(&worktree, &["push", "-q", "-u", "origin", "aspen"]);
    let mut watching = Watching::start(&worktree).await;
    watching.next().await;
    watching.git(&["commit", "-q", "--allow-empty", "-m", "second"]);
    let committed = watching.next().await;

    watching.git(&["push", "-q"]);
    let pushed = watching.next().await;

    let upstream_tip = |branches: &Branches| branches.checked_out()?.upstream.as_ref()?.tip.clone();
    let tip = committed.checked_out().map(|branch| branch.tip.clone());
    assert_ne!(upstream_tip(&committed), tip);
    assert_eq!(upstream_tip(&pushed), tip);
}

/// Where the worktree keeps its HEAD and reflog in a reftable of its own, which a switch rewrites.
#[tokio::test]
async fn it_sends_a_switch_in_a_reftable_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    if !crate::git::branches::tests::reftable(&repo) {
        eprintln!("skipped: this git can't make a reftable");
        return;
    }
    let worktree = dir.path().join("aspen");
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    run(&repo, &["branch", "header"]);
    let mut watching = Watching::start(&worktree).await;
    watching.next().await;

    watching.git(&["switch", "-q", "header"]);

    assert_eq!(watching.next().await.head, Head::Branch { name: BranchName::from("header") });
}
