//! A worktree's watch, on this platform's own watcher and, where one platform's way needs none of its APIs, on the
//! others' too, against real repositories made with a git that reads none of the person's config.

use super::each_folder::EachFolder;
use super::from_the_top::FromTheTop;
use super::heard::{Heard, News};
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
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// What the watch of `worktree` shares with the OS.
async fn heard(worktree: &Path) -> Arc<Heard> {
    heard_of(&given(), worktree).await
}

/// How many folders [`marked`] has made, so each has a name of its own.
static MARKS: AtomicUsize = AtomicUsize::new(0);

/// What was heard before a mark.
struct UpToMark {
    /// The paths that may change the summary, the marks' own left out.
    paths: Vec<PathBuf>,
    news: News,
}

/// Makes a folder at the top of the worktree `heard` is of, which the OS tells of in one event, and waits for it to be
/// heard, so whatever was heard before it has been heard too, and takes what was. The wake the mark brings is used up,
/// so what's heard next is only of what comes after.
///
/// The OS tells of what it watches in the order it happened, but for Windows across the folders that each have a watch
/// of their own, where nothing runs these. A file's close is one exception: it happens once every process holding the
/// file has closed it, and one forked while it was open, as for git in a test beside this one, holds it until it starts
/// git. So a file written before a mark can be told of again after it, and a test looks for what it wrote, rather than
/// for nothing at all.
async fn marked(heard: &Heard) -> UpToMark {
    let mark = format!("{MARK}{}", MARKS.fetch_add(1, Ordering::Relaxed));
    std::fs::create_dir(heard.root.join(&mark)).unwrap();
    waiting::task::until(PATIENCE, format!("{mark} to be heard"), || {
        heard.paths.lock().iter().any(|path| path.file_name() == Some(OsStr::new(&mark)))
    })
    .await;
    let paths = std::mem::take(&mut *heard.paths.lock());
    let marks = |path: &PathBuf| path.file_name().is_some_and(|name| name.to_string_lossy().starts_with(MARK));
    let paths = paths.into_iter().filter(|path| !marks(path)).collect();
    // What the mark woke, which nothing waits for.
    let _ = tokio::time::timeout(Duration::ZERO, heard.wake.notified()).await;
    UpToMark { paths, news: heard.take() }
}

/// How each folder [`marked`] makes is named.
const MARK: &str = "zz-mark-";

/// Whether any of `paths` is `wanted`, from the top of the worktree.
fn named(paths: &[PathBuf], wanted: &str) -> bool {
    paths.iter().any(|path| path.ends_with(wanted))
}

/// Those of `paths` in the folder `folder` of the worktree `heard` is of.
fn in_folder<'a>(heard: &Heard, paths: &'a [PathBuf], folder: &str) -> Vec<&'a PathBuf> {
    paths.iter().filter(|path| path.starts_with(heard.root.join(folder))).collect()
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
        marked(&heard).await;
        for n in 0..200 {
            write(&worktree.join(format!("ignored/deep/{n}.o")), "built");
        }
        let built = marked(&heard).await;
        write(&worktree.join("src/deep/b.rs"), &format!("fn b() {{ /* {way} */ }}\n"));
        let edited = marked(&heard).await;

        assert_eq!(
            in_folder(&heard, &built.paths, "ignored"),
            [] as [&PathBuf; 0],
            "{way}: a build into ignored/ woke it"
        );
        assert!(named(&edited.paths, "src/deep/b.rs"), "{way}: an edit to src/deep/b.rs didn't");
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
    let git_dir = heard.git_dir.clone().expect("git said where its directory is");

    for way in WAYS {
        let _watching = watching(&heard, way);
        marked(&heard).await;
        run(&worktree, &["add", "new.rs"]);
        let staged = marked(&heard).await;
        run(&worktree, &["reset", "-q", "new.rs"]);
        marked(&heard).await;

        assert!(staged.paths.contains(&git_dir.join("index")), "{way}: staging didn't wake it: {:?}", staged.paths);
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
        marked(&heard).await;
        let top = format!("pkg-{}", way.replace(' ', "-"));
        let made = worktree.join(&top);
        std::fs::create_dir_all(made.join("inner")).unwrap();
        std::fs::create_dir_all(made.join("build/out")).unwrap();
        let news = marked(&heard).await.news;
        take_in(&given(), &worktree, &heard, &mut os, news).await;

        write(&made.join("build/out/x.o"), "built");
        let built = marked(&heard).await;
        write(&made.join("inner/c.rs"), "fn c() {}\n");
        let edited = marked(&heard).await;

        assert!(os.is_some(), "{way}: it gave up its watch");
        let into_build = in_folder(&heard, &built.paths, &format!("{top}/build"));
        assert_eq!(into_build, [] as [&PathBuf; 0], "{way}: a build into a new build/ woke it");
        assert!(named(&edited.paths, "inner/c.rs"), "{way}: an edit in a new folder's folder didn't");
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
    marked(&heard).await;

    write(&worktree.join(".gitignore"), "ignored/\nbuild/\nlogs/\n");
    let changed = marked(&heard).await.news;
    let ignores_changed = changed.ignores_changed;
    take_in(&given(), &worktree, &heard, &mut os, changed).await;
    marked(&heard).await;
    write(&worktree.join("logs/today.log"), "a line\n");
    let logged = marked(&heard).await;

    assert!(ignores_changed, "the .gitignore wasn't heard to change");
    assert_eq!(
        in_folder(&heard, &logged.paths, "logs"),
        [] as [&PathBuf; 0],
        "writing into logs/, ignored since, woke it"
    );
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
        marked(&heard).await;
        write(&worktree.join("src/deep/b.rs"), &format!("fn b() {{ /* {way} */ }}\n"));
        let named = marked(&heard).await.paths;

        assert!(!named.is_empty(), "{way}: the edit wasn't heard");
        assert!(named.iter().all(|path| path.ends_with("src/deep/b.rs")), "{way}: named {named:?}");
    }
}

/// A watch on each folder that would need more than it may hold holds none, and one that's asked to watch a folder
/// past that says so, so the loop lets go of the OS's watch, rather than taking the watches the person's editor and
/// the other workers share, and reads every ten seconds instead of hearing.
#[tokio::test]
async fn a_watch_of_more_folders_than_it_may_hold_holds_none_and_reads_on_a_timer() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let heard = heard(&worktree).await;
    // The top, src/, src/deep/ and the worktree's git directory, but not ignored/ or what's in it.
    let needs = 4;

    let too_few = EachFolder::start(&heard, needs - 1);
    let just_enough = EachFolder::start(&heard, needs).expect("watching as many as it may");
    let mut os = Some(Box::new(just_enough) as Box<dyn Watching>);
    let while_watched = settling(os.as_deref());
    std::fs::create_dir_all(worktree.join("more")).unwrap();
    let news = News { folders_made: vec![heard.root.join("more")], ..News::default() };
    take_in(&given(), &worktree, &heard, &mut os, news).await;

    assert!(too_few.is_none());
    assert!(os.is_none(), "it kept a watch that can't hold every folder");
    // The OS's watcher holds what it tells until it has let go.
    waiting::task::until(PATIENCE, "the OS's watcher to let go", || Arc::strong_count(&heard) == 1).await;
    assert_eq!(while_watched.at_least_every, Duration::from_mins(5));
    assert_eq!(settling(os.as_deref()).at_least_every, Duration::from_secs(10));
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

/// Files already in a folder as it appears, as in one moved into the worktree, or written into one as soon as it's
/// made, are in the next summary, though the OS tells only of the folder, and what's written in the folders in it later
/// is heard too.
#[tokio::test]
async fn a_file_in_a_new_folder_is_in_the_next_summary() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path()).await;
    let mut watched = Watched::start(&worktree).await;
    watched.next(PATIENCE).await.expect("the first summary");
    let made_elsewhere = dir.path().join("made-elsewhere");
    for n in 0..5 {
        write(&made_elsewhere.join(format!("deeper/{n}.rs")), "fn f() {}\n");
    }

    std::fs::rename(&made_elsewhere, worktree.join("fresh")).unwrap();
    let summary = watched.next(PATIENCE).await.expect("a summary after the folder");
    write(&worktree.join("fresh/deeper/late.rs"), "fn late() {}\n");
    let late = watched.next(PATIENCE).await.expect("a summary after a later file");

    let listed: Vec<&str> = summary.files.iter().map(|file| file.path.as_str()).collect();
    let moved_in =
        ["fresh/deeper/0.rs", "fresh/deeper/1.rs", "fresh/deeper/2.rs", "fresh/deeper/3.rs", "fresh/deeper/4.rs"];
    assert_eq!(listed, moved_in);
    assert!(summary.files.iter().all(|file| file.status == ChangeStatus::Untracked), "{:?}", summary.files);
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
