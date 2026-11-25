//! The OS's watcher of a worktree, which each platform has its own way of, behind one trait, so the loop reads the same
//! everywhere. Linux watches each folder git doesn't ignore, and macOS and Windows watch from the top.

use super::heard::Heard;
use notify::{Config, RecommendedWatcher, Watcher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

/// A worktree's watch by the OS, which tells `Heard` what it hears until it's dropped.
pub(super) trait Watching: Send {
    /// Watches each of `folders`, which have appeared in the worktree and which git doesn't ignore. Fails once that's
    /// more than the watch may hold, which leaves it watching less than the worktree, so the loop gives it up.
    fn add(&mut self, folders: &[PathBuf]) -> Result<(), TooMany>;
    /// Stops watching each of `folders`, which have gone or moved away.
    fn forget(&mut self, folders: &[PathBuf]);
}

/// More folders than the watch may hold.
#[derive(Debug)]
pub(super) struct TooMany;

/// The OS's watch of the worktree `heard` is of, as this platform watches one, started off the async threads, since
/// it reads the worktree's folders: `None` where the OS won't watch it, which the loop does without.
pub(super) async fn start(heard: &Arc<Heard>) -> Option<Box<dyn Watching>> {
    let heard = heard.clone();
    match tokio::task::spawn_blocking(move || started(&heard)).await {
        Ok(watching) => watching,
        Err(e) => {
            warn!(
                name: "changes.watch_stopped",
                error = %e,
                "Starting the OS's watch of a worktree stopped, so it's read on a timer"
            );
            None
        }
    }
}

/// The OS's watch of the worktree `heard` is of, as Linux watches one: `None` where the OS won't watch it, as once
/// the user has used up their inotify watches or instances.
#[cfg(target_os = "linux")]
fn started(heard: &Arc<Heard>) -> Option<Box<dyn Watching>> {
    let most = super::each_folder::most_watches();
    super::each_folder::EachFolder::start(heard, most).map(|watching| Box::new(watching) as Box<dyn Watching>)
}

/// The OS's watch of the worktree `heard` is of, as macOS and Windows watch one: `None` where the OS won't watch it.
#[cfg(not(target_os = "linux"))]
fn started(heard: &Arc<Heard>) -> Option<Box<dyn Watching>> {
    super::from_the_top::FromTheTop::start(heard, super::from_the_top::WATCH_EACH_TOP_FOLDER)
        .map(|watching| Box::new(watching) as Box<dyn Watching>)
}

/// The OS's own watcher, telling `heard` what it hears. It follows no link to a folder, since inotify keeps one watch
/// for each folder, so a folder watched under a link's name too is told of under one of the two.
pub(super) fn os_watcher(heard: &Arc<Heard>) -> notify::Result<RecommendedWatcher> {
    let heard = heard.clone();
    RecommendedWatcher::new(move |event| heard.hear(&event), Config::default().with_follow_symlinks(false))
}

/// The folders directly in `folder` that a watch can hold: real folders, not links to one, and not git's own.
pub(super) fn folders_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let is_folder = |entry: &std::fs::DirEntry| entry.file_type().is_ok_and(|kind| kind.is_dir());
    entries
        .flatten()
        .filter(|entry| is_folder(entry) && entry.file_name() != ".git")
        .map(|entry| entry.path())
        .collect()
}

/// The folders directly in each of `folders`, as [`folders_in`] finds them, read off the async threads. None if the
/// reading stopped, which leaves the folders it would have found unwatched, as one made after a watch had to give up.
pub(super) async fn folders_in_each(folders: Vec<PathBuf>) -> Vec<PathBuf> {
    let reading = tokio::task::spawn_blocking(move || folders.iter().flat_map(|folder| folders_in(folder)).collect());
    reading.await.unwrap_or_else(|e| {
        warn!(
            name: "changes.new_folders_unread",
            error = %e,
            "Reading the folders in a worktree's new folders stopped, so they aren't watched"
        );
        Vec::new()
    })
}

/// Unwatches each of `folders` that `watcher` watches, as a folder that's gone is.
pub(super) fn unwatch(watcher: &mut RecommendedWatcher, folders: &[PathBuf]) {
    for folder in folders {
        // One the OS dropped itself, as inotify does a folder deleted, isn't watched by now.
        let _ = watcher.unwatch(folder);
    }
}
