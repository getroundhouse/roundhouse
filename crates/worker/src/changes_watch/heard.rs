//! What the OS says of a watched worktree, which the watch's loop reads: whether it may change the summary, which
//! folders appeared or went, and whether the watch lost track.

use super::ignored::Ignored;
use crate::settling::Told;
use notify::event::{ModifyKind, RemoveKind};
use notify::{Event, EventKind};
use parking_lot::{Mutex, RwLock};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use tokio::sync::Notify;

/// The files in a worktree's own git directory that change its summary: the index, as a file is added or taken out of
/// it, and `HEAD`, as a commit is checked out, which can change the base. git writes each by renaming a lock onto it.
const GIT_STATE: [&str; 2] = ["index", "HEAD"];

/// What the OS's watcher and the loop that reads share.
pub(super) struct Heard {
    /// The worktree, as the OS names it in what it says, with every link in it followed, as macOS names `/tmp` as
    /// `/private/tmp`.
    pub(super) root: PathBuf,
    /// The worktree's own git directory, named as the OS does, if git said where it is.
    pub(super) git_dir: Option<PathBuf>,
    pub(super) ignored: RwLock<Ignored>,
    pub(super) news: Mutex<News>,
    /// Holds one wake for the loop, however much was heard while it was reading.
    pub(super) wake: Notify,
    /// Every path it's heard of that may change the summary, for a test, to see which path the OS names, and what it
    /// heard before a path the test waits for.
    #[cfg(test)]
    pub(super) paths: Mutex<Vec<PathBuf>>,
}

/// What was heard since the loop last looked, besides that something changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct News {
    /// Folders that appeared, by their full paths, which git may or may not ignore.
    pub(super) folders_made: Vec<PathBuf>,
    /// Folders that went, or moved away, by their full paths.
    pub(super) folders_gone: Vec<PathBuf>,
    /// A `.gitignore` changed, so what git ignores may have.
    pub(super) ignores_changed: bool,
    /// The OS dropped what it had to say, as when its queue fills, or the watcher failed, so the watch starts again.
    pub(super) lost_track: bool,
}

impl Heard {
    pub(super) fn new(root: PathBuf, git_dir: Option<PathBuf>, ignored: Ignored) -> Self {
        Self {
            root,
            git_dir,
            ignored: RwLock::new(ignored),
            news: Mutex::new(News::default()),
            wake: Notify::new(),
            #[cfg(test)]
            paths: Mutex::new(Vec::new()),
        }
    }

    /// What was heard since the last time, taken, so the next look has only what's heard after it.
    pub(super) fn take(&self) -> News {
        std::mem::take(&mut self.news.lock())
    }

    /// Takes in what the OS said, waking the loop if it may change the summary, as [`Told`] says. What git ignores
    /// wakes nothing either, so a build writing into `target/` costs no summaries.
    pub(super) fn hear(&self, event: &notify::Result<Event>) {
        let event = match Told::of(event) {
            Told::LostTrack => {
                self.news.lock().lost_track = true;
                self.wake.notify_one();
                return;
            }
            Told::NothingWritten => return,
            Told::Changed(event) => event,
        };
        let mut matters = false;
        for path in &event.paths {
            match self.place(path) {
                Place::Elsewhere => {}
                Place::GitState => {
                    matters = true;
                    self.keep_for_a_test(path);
                }
                Place::Worktree(from_top) => {
                    matters = true;
                    self.keep_for_a_test(path);
                    self.note(event.kind, path, &from_top);
                }
            }
        }
        if matters {
            self.wake.notify_one();
        }
    }

    /// Where `path` is, for what's heard of it.
    fn place(&self, path: &Path) -> Place {
        if let Some(git_dir) = &self.git_dir {
            if path.parent() == Some(git_dir)
                && path.file_name().is_some_and(|name| GIT_STATE.map(OsStr::new).contains(&name))
            {
                return Place::GitState;
            }
            if path.starts_with(git_dir) {
                return Place::Elsewhere;
            }
        }
        let Ok(from_top) = path.strip_prefix(&self.root) else { return Place::Elsewhere };
        let in_git = from_top.components().any(|part| part == Component::Normal(OsStr::new(".git")));
        if from_top.as_os_str().is_empty() || in_git || self.ignored.read().covers(from_top) {
            return Place::Elsewhere;
        }
        Place::Worktree(from_top.to_path_buf())
    }

    /// Notes what a change at `path`, which git doesn't ignore, says besides that something changed.
    fn note(&self, kind: EventKind, path: &Path, from_top: &Path) {
        let mut news = self.news.lock();
        if from_top.file_name() == Some(OsStr::new(".gitignore")) {
            news.ignores_changed = true;
        }
        match kind {
            EventKind::Remove(RemoveKind::Folder) => news.folders_gone.push(path.to_path_buf()),
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_)) => {
                // Unfollowed, since a link to a folder is a file to git, and watching what it names could watch a
                // folder twice, or one outside the worktree.
                if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
                    news.folders_made.push(path.to_path_buf());
                } else if matches!(kind, EventKind::Modify(ModifyKind::Name(_))) {
                    // Moved away, or renamed from, which can be a folder that's watched.
                    news.folders_gone.push(path.to_path_buf());
                }
            }
            EventKind::Any | EventKind::Access(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Other => {}
        }
    }
}

impl Heard {
    /// Keeps that it heard of `path`, for a test.
    #[cfg(test)]
    fn keep_for_a_test(&self, path: &Path) {
        self.paths.lock().push(path.to_path_buf());
    }

    /// Keeps nothing outside a test.
    #[cfg(not(test))]
    #[expect(clippy::unused_self, reason = "a test's version keeps it on `self`")]
    fn keep_for_a_test(&self, _path: &Path) {}
}

/// Where a path the OS names is, as the watch sees it.
enum Place {
    /// Outside the worktree, in git's own files, or ignored.
    Elsewhere,
    /// One of [`GIT_STATE`].
    GitState,
    /// In the worktree, where git doesn't ignore it, from the top of the worktree.
    Worktree(PathBuf),
}
