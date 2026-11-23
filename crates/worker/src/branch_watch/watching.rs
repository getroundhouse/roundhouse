use crate::git::branches;
use crate::git::branches::{Places, Read};
use crate::serve::Outbox;
use crate::settling::{Settling, Woke};
use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use process::Environment;
use protocol::model::Branches;
use protocol::worker::WorkerFrame;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, oneshot, watch};
use tracing::{debug, warn};

/// How the branches wait for git to stop writing: 100 ms, short enough that the hub hears of a push at once, with no
/// limit, since git writes a ref in a moment, and every five minutes whatever the OS has said.
const SETTLING: Settling =
    Settling { quiet: Duration::from_millis(100), at_most: None, at_least_every: Duration::from_mins(5) };

/// How often it looks again for the worktree's repository while it can't find it, which is only while something's
/// wrong with the worktree.
const LOOK_FOR_THE_REPOSITORY_EVERY: Duration = Duration::from_mins(1);

/// How often the watcher that looks at the files itself does, where the OS's own can't be had, such as once a Linux
/// user has used up their inotify instances.
const POLL_EVERY: Duration = Duration::from_secs(1);

/// Whose branches a watch is of, which says which frame they go in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Whose {
    /// The workspace's own worktree's.
    Own,
    /// Another worktree's, made for the workspace's work.
    Child,
}

impl Whose {
    pub(super) fn frame(self, worktree: &Path, branches: Branches) -> WorkerFrame {
        match self {
            Self::Own => WorkerFrame::Branches { branches },
            Self::Child => WorkerFrame::ChildBranches { path: worktree.to_string_lossy().into_owned(), branches },
        }
    }
}

/// What the OS's watcher and the loop that reads share.
pub(super) struct Wakes {
    pub(super) places: Places,
    /// The last read, which says which refs' files matter.
    pub(super) last: Mutex<Option<Read>>,
    /// Holds one wake for the loop, however many changes came while it was reading.
    pub(super) wake: Notify,
}

/// Sends what the worktree's branches are at once, and again each time they change, to whichever hub is connected,
/// until the worker stops or another watch replaces this one. `first` is told once the first is sent, or can't be.
/// What's sent while no hub is connected is lost, since the next hub to connect watches them again.
pub(crate) async fn watch(
    given: watch::Receiver<Arc<Environment>>,
    worktree: PathBuf,
    whose: Whose,
    outbox: Arc<Outbox>,
    mut stopping: watch::Receiver<bool>,
    first: oneshot::Sender<()>,
) {
    let mut first = Some(first);
    let Some(places) = repository(&given, &worktree, &mut stopping, &mut first).await else { return };
    let wakes = Arc::new(Wakes { places, last: Mutex::new(None), wake: Notify::new() });
    // Watching before the first read, so a change made while it reads is heard.
    let _watching = start(&wakes);
    loop {
        // Each read with what's given then, so a credential set since reaches it.
        let now = given.borrow().clone();
        read_and_send(&now, &worktree, whose, &wakes, &outbox).await;
        if let Some(first) = first.take() {
            let _ = first.send(());
        }
        if SETTLING.wait(&wakes.wake, &mut stopping).await == Woke::Stopping {
            return;
        }
    }
}

/// The worktree's repository, looked for until it's found or the worker stops. `first` is told if it isn't found at
/// once, since then there's nothing to send.
#[expect(
    clippy::disallowed_methods,
    reason = "how often a worktree whose repository can't be found yet is looked at again"
)]
async fn repository(
    given: &watch::Receiver<Arc<Environment>>,
    worktree: &Path,
    stopping: &mut watch::Receiver<bool>,
    first: &mut Option<oneshot::Sender<()>>,
) -> Option<Places> {
    loop {
        let given = given.borrow().clone();
        match branches::places(&given, worktree).await {
            Ok(places) => return Some(places),
            Err(e) => debug!(worktree = %worktree.display(), "finding the worktree's repository: {}", e.message),
        }
        if let Some(first) = first.take() {
            let _ = first.send(());
        }
        tokio::select! {
            () = tokio::time::sleep(LOOK_FOR_THE_REPOSITORY_EVERY) => {}
            _ = stopping.wait_for(|stopping| *stopping) => return None,
        }
    }
}

/// The OS's own watcher, or one that looks at the files itself where that can't be had. `None` when neither can,
/// which leaves only the reads every few minutes.
pub(super) fn start(wakes: &Arc<Wakes>) -> Option<Box<dyn Watcher + Send>> {
    let heard = {
        let wakes = wakes.clone();
        move |event: notify::Result<Event>| {
            if mattered(&wakes, &event) {
                wakes.wake.notify_one();
            }
        }
    };
    let own = notify::recommended_watcher(heard.clone()).and_then(|watcher| watching(watcher, &wakes.places));
    match own {
        Ok(watcher) => return Some(Box::new(watcher)),
        Err(e) => warn!("the OS can't say when git changes the branches, so they're looked at instead: {e}"),
    }
    let config = notify::Config::default().with_poll_interval(POLL_EVERY);
    match PollWatcher::new(heard, config).and_then(|watcher| watching(watcher, &wakes.places)) {
        Ok(watcher) => Some(Box::new(watcher)),
        Err(e) => {
            warn!("the branches can't be watched, so they're read every few minutes: {e}");
            None
        }
    }
}

/// The watcher, watching each of [`branches::watched`] that's there.
fn watching<W: Watcher>(mut watcher: W, places: &Places) -> notify::Result<W> {
    for watched in branches::watched(places).into_iter().filter(|watched| watched.directory.is_dir()) {
        let depth = if watched.deep { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        watcher.watch(&watched.directory, depth)?;
    }
    Ok(watcher)
}

/// Whether what the watcher heard may have changed the branches. One it lost track with, or an error, may have. A file
/// opened, read or closed without being written to hasn't, which inotify tells of too, and every git command opens
/// `HEAD` and the config, so counting those, the watcher's own reads would wake it again, and every other worker's on
/// the same repository, for ever.
pub(super) fn mattered(wakes: &Wakes, event: &notify::Result<Event>) -> bool {
    let Ok(event) = event else { return true };
    if event.need_rescan() {
        return true;
    }
    if let EventKind::Access(access) = event.kind
        && access != AccessKind::Close(AccessMode::Write)
    {
        return false;
    }
    let last = wakes.last.lock();
    event.paths.iter().any(|file| branches::matters(&wakes.places, last.as_ref(), file))
}

/// Reads the branches and sends them if they've changed.
async fn read_and_send(given: &Environment, worktree: &Path, whose: Whose, wakes: &Wakes, outbox: &Outbox) {
    let read = match branches::read(given, worktree, &wakes.places).await {
        Ok(read) => read,
        Err(e) => {
            debug!(worktree = %worktree.display(), "reading the branches: {}", e.message);
            return;
        }
    };
    let news = {
        let mut last = wakes.last.lock();
        let news = last.as_ref().is_none_or(|last| last.branches != read.branches).then(|| read.branches.clone());
        *last = Some(read);
        news
    };
    if let Some(branches) = news {
        outbox.send(whose.frame(worktree, branches)).await;
    }
}
