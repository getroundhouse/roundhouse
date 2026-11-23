//! The OS's watcher of a worktree, which each platform has its own way of, behind one trait, so the loop reads the same
//! everywhere. Linux watches each folder git doesn't ignore, and macOS and Windows watch from the top.

use super::heard::Heard;
use notify::{Config, RecommendedWatcher, Watcher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// The OS's watch of the worktree `heard` is of, as this platform watches one: `None` where the OS won't watch it,
/// as once a Linux user has used up their inotify watches or instances, which the loop does without.
#[cfg(target_os = "linux")]
pub(super) fn start(heard: &Arc<Heard>) -> Option<Box<dyn Watching>> {
    let most = super::each_folder::most_watches();
    super::each_folder::EachFolder::start(heard, most).map(|watching| Box::new(watching) as Box<dyn Watching>)
}

/// The OS's watch of the worktree `heard` is of, as this platform watches one: `None` where the OS won't watch it,
/// which the loop does without.
#[cfg(not(target_os = "linux"))]
pub(super) fn start(heard: &Arc<Heard>) -> Option<Box<dyn Watching>> {
    super::from_the_top::FromTheTop::start(heard, super::from_the_top::WATCH_EACH_TOP_FOLDER)
        .map(|watching| Box::new(watching) as Box<dyn Watching>)
}

/// The OS's own watcher, telling `heard` what it hears. It follows no link to a folder, since inotify keeps one watch
/// for each folder, so a folder watched under a link's name too is told of under one of the two.
pub(super) fn os_watcher(heard: &Arc<Heard>) -> notify::Result<RecommendedWatcher> {
    let heard = heard.clone();
    RecommendedWatcher::new(move |event| heard.hear(event), Config::default().with_follow_symlinks(false))
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

/// Unwatches each of `folders` that `watcher` watches, as a folder that's gone is.
pub(super) fn unwatch(watcher: &mut RecommendedWatcher, folders: &[PathBuf]) {
    for folder in folders {
        // One the OS dropped itself, as inotify does a folder deleted, isn't watched by now.
        let _ = watcher.unwatch(folder);
    }
}
