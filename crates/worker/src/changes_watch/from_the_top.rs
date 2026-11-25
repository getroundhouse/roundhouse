//! macOS's and Windows' watch: one watch from the top of the worktree that tells of every folder in it, with what git
//! ignores dropped as it's heard. `FSEvents` watches a whole tree for about the same at any size, and notify can't give
//! it thousands of folders one by one, and Windows watches a folder and all in it with one handle.

use super::heard::Heard;
use super::system::{TooMany, Watching, folders_in, os_watcher, unwatch};
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

/// Whether each folder at the top of the worktree has a watch of its own, with the top itself watched alone, rather
/// than one watch on the top that holds everything. Windows does, since notify gives each of its watches a buffer of 16
/// KB, and when a build's burst into `target/` fills one, Windows says so with an error that notify doesn't know, and
/// notify ends that watch without saying. So `target/`, and a `node_modules/` at the top, aren't watched at all there.
/// `FSEvents` has no such buffer.
#[cfg_attr(
    target_os = "linux",
    expect(dead_code, reason = "Linux watches each folder, and builds this only for tests")
)]
pub(super) const WATCH_EACH_TOP_FOLDER: bool = cfg!(windows);

/// A watch from the top of a worktree.
pub(super) struct FromTheTop {
    watcher: RecommendedWatcher,
    root: PathBuf,
    each_top_folder: bool,
}

impl FromTheTop {
    /// Watches the worktree `heard` is of, and its git directory, from the top, or, with `each_top_folder`, each folder
    /// at the top that git doesn't ignore, as [`WATCH_EACH_TOP_FOLDER`] says. `None` if the OS won't.
    pub(super) fn start(heard: &Arc<Heard>, each_top_folder: bool) -> Option<Self> {
        match Self::watching(heard, each_top_folder) {
            Ok(watching) => Some(watching),
            Err(e) => {
                warn!(
                    name: "changes.watch_failed",
                    worktree = %heard.root.display(),
                    error = %e,
                    "The OS can't watch a worktree, so it's read on a timer"
                );
                None
            }
        }
    }

    fn watching(heard: &Arc<Heard>, each_top_folder: bool) -> notify::Result<Self> {
        let mut watcher = os_watcher(heard)?;
        let mut paths = watcher.paths_mut();
        if each_top_folder {
            paths.add(&heard.root, RecursiveMode::NonRecursive)?;
            let ignored = heard.ignored.read();
            for folder in folders_in(&heard.root) {
                if folder.strip_prefix(&heard.root).is_ok_and(|from_top| !ignored.covers(from_top)) {
                    paths.add(&folder, RecursiveMode::Recursive)?;
                }
            }
        } else {
            paths.add(&heard.root, RecursiveMode::Recursive)?;
        }
        if let Some(git_dir) = heard.git_dir.as_ref().filter(|git_dir| !git_dir.starts_with(&heard.root)) {
            paths.add(git_dir, RecursiveMode::NonRecursive)?;
        }
        paths.commit()?;
        Ok(Self { watcher, root: heard.root.clone(), each_top_folder })
    }
}

impl Watching for FromTheTop {
    /// A folder new at the top gets a watch of its own where each has one, and any other is in a watch already.
    fn add(&mut self, folders: &[PathBuf]) -> Result<(), TooMany> {
        if !self.each_top_folder {
            return Ok(());
        }
        for folder in folders.iter().filter(|folder| folder.parent() == Some(self.root.as_path())) {
            // One gone again already is heard as it goes.
            let _ = self.watcher.watch(folder, RecursiveMode::Recursive);
        }
        Ok(())
    }

    fn forget(&mut self, folders: &[PathBuf]) {
        if self.each_top_folder {
            let top: Vec<PathBuf> =
                folders.iter().filter(|folder| folder.parent() == Some(self.root.as_path())).cloned().collect();
            unwatch(&mut self.watcher, &top);
        }
    }
}
