//! A worktree's watch, on this platform's own watcher and, where one platform's way needs none of its APIs, on the
//! others' too, against real repositories made with a git that reads none of the person's config.

use super::each_folder::EachFolder;
use super::from_the_top::FromTheTop;
use super::heard::Heard;
use super::system;
use super::system::Watching;
use super::watching::*;
use crate::git::ChangeSummaries;
use crate::git::tests::{new_repo, prepare, run};
use crate::serve::Outbox;
use crate::serve::Outgoing;
use crate::testing::given;
use protocol::model::ChangeStatus;
use protocol::model::ChangeSummary;
use protocol::worker::WorkerFrame;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::watch;

/// Long enough for a busy machine, and only waited out when the watch has failed to send.
const PATIENCE: Duration = Duration::from_secs(20);

/// How long a watch has to stay asleep to count as not woken, which is waited out each time.
const QUIET: Duration = Duration::from_millis(500);

/// Each way a platform watches, so each is tested here whichever platform runs the tests: Linux's, a watch on each
/// folder, and macOS's and Windows', from the top, as one watch or one for each folder at the top.
const WAYS: [&str; 3] = ["each folder", "from the top", "each folder at the top"];

/// A watch of the worktree `heard` is of, the way `way` names.
fn watching(heard: &Arc<Heard>, way: &str) -> Box<dyn Watching> {
    match way {
        "each folder" => Box::new(EachFolder::start(heard, usize::MAX).unwrap()),
        "from the top" => Box::new(FromTheTop::start(heard, false).unwrap()),
        _ => Box::new(FromTheTop::start(heard, true).unwrap()),
    }
}

/// A workspace's worktree of a repository that ignores `ignored/` and `build/` wherever it is, with `src/` tracked.
async fn worktree(dir: &Path) -> PathBuf {
    let repo = new_repo(dir);
    std::fs::write(repo.join(".gitignore"), "ignored/\nbuild/\n").unwrap();
    std::fs::create_dir_all(repo.join("src/deep")).unwrap();
    std::fs::write(repo.join("src/deep/b.rs"), "fn b() {}\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "src"]);
    let prepared = prepare(&dir.join("ws"), &repo, "alder", None).await.unwrap();
    std::fs::create_dir_all(Path::new(&prepared.path).join("ignored/deep")).unwrap();
    PathBuf::from(prepared.path)
}

/// What the watch of `worktree` shares with the OS, once anything it heard before has come and gone.
async fn heard(worktree: &Path) -> Arc<Heard> {
    heard_of(&given(), worktree).await
}

/// Drops whatever was heard so far, such as the writes that made the worktree, once it's gone quiet.
async fn settled(heard: &Heard) {
    while woke(heard, QUIET).await {}
    heard.take();
}

async fn woke(heard: &Heard, within: Duration) -> bool {
    tokio::time::timeout(within, heard.wake.notified()).await.is_ok()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A build writing into a folder git ignores wakes nothing, whichever way the worktree is watched, and an edit to a
/// file git tracks does, so a build costs no summaries.
#[tokio::test]
async fn an_ignored_folder_wakes_no_summary() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let heard = heard(&worktree).await;

    for way in WAYS {
        let _watching = watching(&heard, way);
        settled(&heard).await;
        for n in 0..200 {
            write(&worktree.join(format!("ignored/deep/{n}.o")), "built");
        }
        let built = woke(&heard, QUIET).await;
        write(&worktree.join("src/deep/b.rs"), &format!("fn b() {{ /* {way} */ }}\n"));
        let edited = woke(&heard, PATIENCE).await;

        assert!(!built, "{way}: a build into ignored/ woke it");
        assert!(edited, "{way}: an edit to src/deep/b.rs didn't");
    }
}

/// What makes the summary's index and HEAD change, which are in the worktree's git directory outside it, wakes it,
/// as staging a file does.
#[tokio::test]
async fn staging_a_file_wakes_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    write(&worktree.join("new.rs"), "fn new() {}\n");
    let heard = heard(&worktree).await;

    for way in WAYS {
        let _watching = watching(&heard, way);
        settled(&heard).await;
        run(&worktree, &["add", "new.rs"]);
        let staged = woke(&heard, PATIENCE).await;
        run(&worktree, &["reset", "-q", "new.rs"]);

        assert!(staged, "{way}: staging didn't wake it");
    }
}

/// A folder that appears is watched once git is found not to ignore it, with each folder in it, and one it ignores, as
/// `build/` anywhere is here, isn't, so what's written there afterwards wakes nothing.
#[tokio::test]
async fn a_new_folder_is_watched_unless_git_ignores_it() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let heard = heard(&worktree).await;

    for way in WAYS {
        let mut os = Some(watching(&heard, way));
        settled(&heard).await;
        let made = worktree.join(format!("pkg-{}", way.replace(' ', "-")));
        std::fs::create_dir_all(made.join("inner")).unwrap();
        std::fs::create_dir_all(made.join("build/out")).unwrap();
        waiting::task::until(PATIENCE, format!("{way}: the new folder to be heard"), || {
            !heard.news.lock().folders_made.is_empty()
        })
        .await;
        settled_taking_in(&worktree, &heard, &mut os).await;

        write(&made.join("build/out/x.o"), "built");
        let built = woke(&heard, QUIET).await;
        write(&made.join("inner/c.rs"), "fn c() {}\n");
        let edited = woke(&heard, PATIENCE).await;

        assert!(os.is_some(), "{way}: it gave up its watch");
        assert!(!built, "{way}: a build into a new build/ woke it");
        assert!(edited, "{way}: an edit in a new folder's folder didn't");
    }
}

/// Takes in what was heard, as the loop does before each summary, until it's gone quiet.
async fn settled_taking_in(worktree: &Path, heard: &Arc<Heard>, os: &mut Option<Box<dyn Watching>>) {
    loop {
        take_in(&given(), worktree, heard, os, heard.take()).await;
        if !woke(heard, QUIET).await {
            return;
        }
    }
}

/// A `.gitignore` changed has the watch start again with what git ignores now, so a folder newly ignored wakes
/// nothing.
#[tokio::test]
async fn a_gitignore_changed_changes_what_wakes_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    std::fs::create_dir_all(worktree.join("logs")).unwrap();
    let heard = heard(&worktree).await;
    let mut os = system::start(&heard);
    settled(&heard).await;

    write(&worktree.join(".gitignore"), "ignored/\nbuild/\nlogs/\n");
    waiting::task::until(PATIENCE, "the .gitignore to be heard", || heard.news.lock().ignores_changed).await;
    settled_taking_in(&worktree, &heard, &mut os).await;
    write(&worktree.join("logs/today.log"), "a line\n");
    let logged = woke(&heard, QUIET).await;

    assert!(!logged, "writing into logs/, ignored since, woke it");
}

/// A folder git tracks that a link in the worktree also reaches is heard of by its own path, whichever way it's
/// watched, since no watch follows a link. inotify keeps one watch for each folder, so a watch that followed the link
/// would have what's heard named under whichever name it watched last, as `docs/reference/file-watching.md` says of the
/// Linux kernel's tree.
#[cfg(unix)]
#[tokio::test]
async fn a_folder_reached_through_a_link_is_heard_by_its_own_path() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    std::os::unix::fs::symlink("src/deep", worktree.join("zz-link")).unwrap();
    let heard = heard(&worktree).await;

    for way in WAYS {
        let _watching = watching(&heard, way);
        settled(&heard).await;
        heard.paths.lock().clear();
        write(&worktree.join("src/deep/b.rs"), &format!("fn b() {{ /* {way} */ }}\n"));
        assert!(woke(&heard, PATIENCE).await, "{way}: the edit wasn't heard");
        settled(&heard).await;

        let named: Vec<PathBuf> = heard.paths.lock().clone();
        assert!(named.iter().all(|path| path.ends_with("src/deep/b.rs")), "{way}: named {named:?}");
    }
}

/// A watch on each folder that would need more than it may hold holds none, and one that's asked to watch a folder
/// past that says so, so the loop gives it up and reads on a timer, rather than taking the watches the person's editor
/// and the other workers share.
#[tokio::test]
async fn a_watch_of_more_folders_than_it_may_hold_holds_none() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let heard = heard(&worktree).await;
    // The top, src/, src/deep/ and the worktree's git directory, but not ignored/ or what's in it.
    let needs = 4;

    let too_few = EachFolder::start(&heard, needs - 1);
    let mut just_enough = EachFolder::start(&heard, needs).expect("watching as many as it may");
    std::fs::create_dir_all(worktree.join("more")).unwrap();
    let past_it = just_enough.add(&[worktree.join("more")]);

    assert!(too_few.is_none());
    assert!(past_it.is_err());
}

/// What reached the hub through a watch's outbox.
struct Watched {
    frames: mpsc::Receiver<Outgoing>,
    outbox: Arc<Outbox>,
    _stopping: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl Watched {
    async fn start(worktree: &Path) -> Self {
        let (frames, received) = mpsc::channel(16);
        let outbox = Arc::new(Outbox::default());
        outbox.attach(frames, false).await;
        let hub = outbox.connection().await.unwrap();
        let stopping = watch::Sender::new(false);
        let (_, given) = watch::channel(Arc::new(given()));
        let watched = super::Watched { worktree: worktree.to_path_buf(), base_ref: None };
        let watching = watch(given, ChangeSummaries::default(), watched, outbox.clone(), hub, stopping.subscribe());
        let task = tokio::spawn(watching);
        Self { frames: received, outbox, _stopping: stopping, task }
    }

    /// The next summary it sent, once it's been written, so the watch can send the next.
    async fn next(&mut self, within: Duration) -> Option<ChangeSummary> {
        let outgoing = tokio::time::timeout(within, self.frames.recv()).await.ok()??;
        let WorkerFrame::Changes { summary, .. } = &*outgoing.frame else {
            panic!("not a summary: {:?}", outgoing.frame)
        };
        Some(summary.clone())
    }
}

fn status_of(summary: &ChangeSummary, path: &str) -> Option<ChangeStatus> {
    summary.files.iter().find(|file| file.path == path).map(|file| file.status)
}

/// The watch sends the summary at once, and again once a file changes, outside any turn, and sends nothing for a
/// write that leaves the summary as it was, as writing a file's own content back does.
#[tokio::test]
async fn the_watch_sends_a_summary_at_once_and_again_only_when_it_differs() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let mut watched = Watched::start(&worktree).await;

    let first = watched.next(PATIENCE).await.expect("the first summary");
    write(&worktree.join("a.txt"), "one\nand two\n");
    let edited = watched.next(PATIENCE).await.expect("a summary after the edit");
    write(&worktree.join("a.txt"), "one\nand two\n");
    let same = watched.next(QUIET * 2).await;

    assert_eq!(first.files, []);
    assert_eq!(status_of(&edited, "a.txt"), Some(ChangeStatus::Modified));
    assert_eq!(same, None, "the same summary was sent again");
}

/// Files written into a folder as soon as it's made are in the next summary, though the OS can't tell of what was
/// written before the folder was watched, and what's written in it later is heard too.
#[tokio::test]
async fn a_file_in_a_new_folder_is_in_the_next_summary() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let mut watched = Watched::start(&worktree).await;
    watched.next(PATIENCE).await.expect("the first summary");

    for n in 0..5 {
        write(&worktree.join(format!("fresh/deeper/{n}.rs")), "fn f() {}\n");
    }
    let mut summary = watched.next(PATIENCE).await.expect("a summary after the folder");
    while summary.files.len() < 5 {
        summary = watched.next(PATIENCE).await.expect("the files written straight after");
    }
    write(&worktree.join("fresh/deeper/late.rs"), "fn late() {}\n");
    let late = watched.next(PATIENCE).await.expect("a summary after a later file");

    assert_eq!(status_of(&summary, "fresh/deeper/4.rs"), Some(ChangeStatus::Untracked));
    assert_eq!(status_of(&late, "fresh/deeper/late.rs"), Some(ChangeStatus::Untracked));
}

/// A watch ends once the hub that asked for it has gone, at its next read, since the next hub says again what it
/// wants.
#[tokio::test]
async fn a_watch_ends_once_its_hub_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let mut watched = Watched::start(&worktree).await;
    watched.next(PATIENCE).await.expect("the first summary");

    watched.outbox.detach().await;
    write(&worktree.join("a.txt"), "changed once the hub had gone\n");

    tokio::time::timeout(PATIENCE, &mut watched.task).await.expect("the watch ended").unwrap();
}
