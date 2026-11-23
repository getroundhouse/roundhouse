//! The watch's loop: what's heard taken in, the summary worked out and sent if it differs, and the wait for the next.

use super::heard::{Heard, News};
use super::ignored::Ignored;
use super::system::{self, Watching};
use crate::git::{self, ChangeSummaries, path_for_git, path_from_git};
use crate::serve::{HubConnection, Outbox};
use crate::settling::{Settling, Woke};
use process::Environment;
use protocol::model::ChangeSummary;
use protocol::worker::WorkerFrame;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Semaphore, watch};
use tokio::time::Instant;
use tracing::{debug, warn};

/// How the watch waits: until writes have stopped for 100 ms, as a save or a checkout writes several files one after
/// another, or for a second of writes that don't stop, as a long run of edits makes, and at least every five minutes.
const SETTLING: Settling = Settling {
    quiet: Duration::from_millis(100),
    at_most: Some(Duration::from_secs(1)),
    at_least_every: Duration::from_mins(5),
};

/// How the watch reads where the OS won't watch the worktree, as once a Linux user has used up their inotify watches:
/// every ten seconds, which the hub still counts as keeping the summary current. That's the owner's choice to make,
/// proposed and not yet confirmed. The other choices were the frame saying the summary isn't watched, which the hub
/// sends as of its read, and refusing the watch, which leaves the summary as of each turn's end.
const WHERE_THE_OS_WONT_WATCH: Settling = Settling { at_least_every: Duration::from_secs(10), ..SETTLING };

/// Whether the watch is started again each time it reads every few minutes, since on Windows notify ends a watch
/// whose buffer filled without saying so, as `from_the_top::WATCH_EACH_TOP_FOLDER` says. Elsewhere a watch that's
/// lost track says so.
const WATCH_AGAIN_WHEN_DUE: bool = cfg!(windows);

/// What a worktree's watch reads and sends.
pub(crate) struct Watched {
    /// As the hub named it, which the frames name it as.
    pub(crate) worktree: PathBuf,
    pub(crate) base_ref: Option<String>,
}

/// Sends the summary of `watched`'s worktree at once, and again whenever it differs from the last one sent, to `hub`,
/// until the worker stops, `hub` is no longer connected, or another watch replaces this one, which aborts it.
pub(crate) async fn watch(
    given: watch::Receiver<Arc<Environment>>,
    changes: ChangeSummaries,
    watched: Watched,
    outbox: Arc<Outbox>,
    hub: HubConnection,
    mut stopping: watch::Receiver<bool>,
) {
    let Watched { worktree, base_ref } = watched;
    let first = given.borrow().clone();
    let heard = heard_of(&first, &worktree).await;
    // Watching before the first read, so a change made while it reads is heard.
    let mut os = system::start(&heard);
    let mut last: Option<ChangeSummary> = None;
    // Held until a frame has been written, so a hub slow to read holds up the next summary rather than queueing them.
    let sending = Arc::new(Semaphore::new(1));
    loop {
        let now = given.borrow().clone();
        take_in(&now, &worktree, &heard, &mut os, heard.take()).await;
        if outbox.connection().await != Some(hub) {
            return;
        }
        let began = Instant::now();
        match changes.summarize(&now, &worktree, base_ref.as_deref()).await {
            Ok(summary) if last.as_ref() != Some(&summary) => {
                let Ok(written) = sending.clone().acquire_owned().await else { return };
                let frame =
                    WorkerFrame::Changes { path: worktree.to_string_lossy().into_owned(), summary: summary.clone() };
                if !outbox.send_to(hub, frame, written).await {
                    return;
                }
                last = Some(summary);
            }
            Ok(_) => {}
            Err(e) => debug!(worktree = %worktree.display(), "summing up the worktree's changes: {}", e.message),
        }
        let took = began.elapsed();
        let settling = if os.is_some() { SETTLING } else { WHERE_THE_OS_WONT_WATCH };
        match settling.wait(&heard.wake, &mut stopping).await {
            Woke::Stopping => return,
            Woke::Due if WATCH_AGAIN_WHEN_DUE => {
                // Before the next, so the two don't hold the OS's watches at once.
                drop(os.take());
                os = system::start(&heard);
            }
            Woke::Due | Woke::Heard => {}
        }
        if !paced(began + took * 2, &mut stopping).await {
            return;
        }
    }
}

/// What the watch of the worktree at `worktree` shares with the OS's watcher, as git says it then: which folders and
/// files it ignores, and where its own git directory is. What git can't say leaves the watch hearing more, which
/// costs only summaries that find nothing new.
pub(super) async fn heard_of(given: &Environment, worktree: &Path) -> Arc<Heard> {
    // As the OS names what it tells of, which macOS does with every link followed.
    let named = |path: PathBuf| std::fs::canonicalize(&path).unwrap_or(path);
    let git_dir = match git::ignored::git_dir(given, worktree).await {
        Ok(git_dir) => Some(named(git_dir)),
        Err(e) => {
            debug!(worktree = %worktree.display(), "finding the worktree's git directory: {}", e.message);
            None
        }
    };
    Arc::new(Heard::new(named(worktree.to_path_buf()), git_dir, ignored(given, worktree).await))
}

/// What git ignores in the worktree at `worktree`, or nothing where it can't say.
async fn ignored(given: &Environment, worktree: &Path) -> Ignored {
    match git::ignored::ignored(given, worktree).await {
        Ok(listed) => Ignored::listed(&listed),
        Err(e) => {
            debug!(worktree = %worktree.display(), "listing what git ignores: {}", e.message);
            Ignored::default()
        }
    }
}

/// Takes in what was heard besides that something changed: a watch that lost track, or a `.gitignore` that changed,
/// starts again with what git ignores now, and a folder that appeared is watched, with each folder in it, unless git
/// ignores it. Each folder in it is looked at, a level at a time, wherever the worktree is watched from the top too,
/// since the folders in it can be made before the OS tells of them, and what git ignores among them has to be known
/// for what's heard of them to be dropped. A watch that can't hold them all is given up for reads on a timer.
pub(super) async fn take_in(
    given: &Environment,
    worktree: &Path,
    heard: &Arc<Heard>,
    os: &mut Option<Box<dyn Watching>>,
    news: News,
) {
    let News { folders_made, folders_gone, ignores_changed, lost_track } = news;
    if lost_track || ignores_changed {
        *heard.ignored.write() = ignored(given, worktree).await;
        // Before the next, so the two don't hold the OS's watches at once.
        drop(os.take());
        *os = system::start(heard);
        return;
    }
    let Some(watching) = os else { return };
    watching.forget(&folders_gone);
    let mut level: Vec<PathBuf> = folders_made;
    level.sort();
    level.dedup();
    while !level.is_empty() {
        let kept = not_ignored(given, worktree, heard, level).await;
        if watching.add(&kept).is_err() {
            warn!(worktree = %worktree.display(), "the worktree has more folders than its watch may hold, so it's read on a timer");
            *os = None;
            return;
        }
        level = kept.iter().flat_map(|folder| system::folders_in(folder)).collect();
    }
}

/// Which of `folders`, in the worktree `heard` is of, git doesn't ignore, taking in those it does.
async fn not_ignored(given: &Environment, worktree: &Path, heard: &Heard, folders: Vec<PathBuf>) -> Vec<PathBuf> {
    let from_top: Vec<(PathBuf, PathBuf)> = folders
        .into_iter()
        .filter_map(|folder| Some((folder.strip_prefix(&heard.root).ok()?.to_path_buf(), folder)))
        .filter(|(from_top, _)| !heard.ignored.read().covers(from_top))
        .collect();
    let asked: Vec<Vec<u8>> = from_top.iter().map(|(from_top, _)| path_for_git(from_top)).collect();
    let ignored: Vec<PathBuf> = match git::ignored::which_ignored(given, worktree, &asked).await {
        Ok(ignored) => ignored.iter().map(|path| path_from_git(path)).collect(),
        // Watched, then, which costs only what it hears.
        Err(e) => {
            debug!(worktree = %worktree.display(), "asking git which new folders it ignores: {}", e.message);
            Vec::new()
        }
    };
    let mut kept = Vec::new();
    let mut taken = heard.ignored.write();
    for (from_top, folder) in from_top {
        if ignored.contains(&from_top) {
            taken.add_folder(from_top);
        } else {
            kept.push(folder);
        }
    }
    kept
}

/// Waits until `until`, so the next summary begins no sooner after the last ended than the last took, and a worktree
/// whose summary takes two seconds, as a large one on a Mac does, costs at most half a core while its files go on
/// changing. False once the worker's stopping.
#[expect(clippy::disallowed_methods, reason = "pacing summaries by how long the last one took")]
async fn paced(until: Instant, stopping: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        () = tokio::time::sleep_until(until) => true,
        _ = stopping.wait_for(|stopping| *stopping) => false,
    }
}
