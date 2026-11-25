//! Linux's watch: an inotify watch on each folder git doesn't ignore, and none on what it does, since a watch on each
//! folder of a 300,000-file `node_modules` uses some 150,000 of the user's watches, more than a stock limit gives them,
//! shared with their editor and every other worker. A folder that appears is watched as the loop finds git doesn't
//! ignore it.

use super::heard::Heard;
use super::system::{TooMany, Watching, folders_in, os_watcher, unwatch};
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Where Linux says how many inotify watches each user may hold, all their processes together. Only Linux reads it,
/// where the rest of this module also builds for every platform's tests.
#[cfg(target_os = "linux")]
const MOST_USER_WATCHES: &str = "/proc/sys/fs/inotify/max_user_watches";

/// What a worktree's watch may hold where the limit can't be read: half of the smallest limit a stock kernel gives.
#[cfg(target_os = "linux")]
const MOST_IF_NOT_SAID: usize = 4096;

/// How many folders one worktree's watch may hold: half of what the user may hold in all, so a worktree that needs
/// more than that leaves the rest to the person's editor and the other workers, and is read on a timer instead. A test
/// gives the watch a limit of its own.
#[cfg(target_os = "linux")]
pub(super) fn most_watches() -> usize {
    let said = std::fs::read_to_string(MOST_USER_WATCHES).ok().and_then(|said| said.trim().parse::<usize>().ok());
    said.map_or(MOST_IF_NOT_SAID, |most| most / 2)
}

/// A watch on each folder of a worktree that git doesn't ignore.
pub(super) struct EachFolder {
    watcher: RecommendedWatcher,
    watched: HashSet<PathBuf>,
    most: usize,
}

impl EachFolder {
    /// Watches every folder of the worktree `heard` is of that git doesn't ignore, and its git directory, unless that's
    /// more than `most` folders or the OS won't, when it holds none of them.
    pub(super) fn start(heard: &Arc<Heard>, most: usize) -> Option<Self> {
        let Some(folders) = folders_of(heard, most) else {
            info!(
                name: "changes.watch_too_big",
                worktree = %heard.root.display(),
                most,
                "A worktree has more folders than its watch may hold, so it's read on a timer"
            );
            return None;
        };
        let mut watcher = match os_watcher(heard) {
            Ok(watcher) => watcher,
            Err(e) => {
                warn!(
                    name: "changes.watch_failed",
                    worktree = %heard.root.display(),
                    error = %e,
                    "The OS can't watch a worktree, so it's read on a timer"
                );
                return None;
            }
        };
        let mut paths = watcher.paths_mut();
        for folder in &folders {
            if let Err(e) = paths.add(folder, RecursiveMode::NonRecursive) {
                warn!(
                    name: "changes.watch_failed",
                    worktree = %heard.root.display(),
                    error = %e,
                    "The OS can't watch a worktree's folders, so it's read on a timer"
                );
                return None;
            }
        }
        if let Err(e) = paths.commit() {
            warn!(
                name: "changes.watch_failed",
                worktree = %heard.root.display(),
                error = %e,
                "The OS can't watch a worktree's folders, so it's read on a timer"
            );
            return None;
        }
        Some(Self { watcher, watched: folders.into_iter().collect(), most })
    }
}

impl Watching for EachFolder {
    fn add(&mut self, folders: &[PathBuf]) -> Result<(), TooMany> {
        for folder in folders {
            if self.watched.contains(folder) {
                continue;
            }
            if self.watched.len() >= self.most {
                return Err(TooMany);
            }
            match self.watcher.watch(folder, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    self.watched.insert(folder.clone());
                }
                Err(e) if matches!(e.kind, notify::ErrorKind::MaxFilesWatch) => return Err(TooMany),
                // Gone again already, which is heard as it goes.
                Err(_) => {}
            }
        }
        Ok(())
    }

    fn forget(&mut self, folders: &[PathBuf]) {
        let gone: Vec<PathBuf> = folders.iter().filter(|folder| self.watched.remove(*folder)).cloned().collect();
        unwatch(&mut self.watcher, &gone);
    }
}

/// The worktree's folders that git doesn't ignore, as `heard` knows what it does, its top among them, without
/// following a link, and its own git directory. `None` once that's more than `most`, which it stops reading at, as a
/// `node_modules` git doesn't ignore has it.
fn folders_of(heard: &Heard, most: usize) -> Option<Vec<PathBuf>> {
    let mut folders = vec![heard.root.clone()];
    folders.extend(heard.git_dir.iter().filter(|git_dir| !git_dir.starts_with(&heard.root)).cloned());
    let mut next = 0;
    let ignored = heard.ignored.read();
    while let Some(folder) = folders.get(next).cloned() {
        next += 1;
        if folder.starts_with(&heard.root) {
            let kept = folders_in(&folder)
                .into_iter()
                .filter(|inner| inner.strip_prefix(&heard.root).is_ok_and(|from_top| !ignored.covers(from_top)));
            folders.extend(kept);
        }
        if folders.len() > most {
            return None;
        }
    }
    Some(folders)
}
