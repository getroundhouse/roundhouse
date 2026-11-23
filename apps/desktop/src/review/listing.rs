//! What Changes lists of a workspace's changed files: the files the hub's summary of them names, and how many it
//! leaves out, where the hub sends one, and otherwise the files of the whole diff.

use std::sync::Arc;

use gpui_kit::{Context, SharedString};
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, LineCounts, Observed};

use super::Review;
use super::listed::{Listed, Summary, SummaryFile};
use crate::patch::{FilePatch, FileStatus};
use client_core::workspace_lines::counted;

/// Which of the changed files one is: its path, and whether it's the untracked file there, since one path can be two
/// changed files, as a file taken out of git's index and left in the worktree is, its deletion and the untracked file
/// of its own, as the hub's summary lists them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChangedEntry {
    pub path: SharedString,
    pub untracked: bool,
}

impl ChangedEntry {
    /// Whether it's the file `path` and `untracked` say, of a list where `shared` says whether another file is at
    /// that path. A list where the path is once is of the one file there, whether it's tracked or not, since the whole
    /// diff says which of its files git tracks only where a path is twice, as [`WholeDiff`](super::listed::WholeDiff)
    /// says, so a file the summary lists as untracked is the whole diff's file at its path.
    pub fn is(&self, path: &str, untracked: bool, shared: bool) -> bool {
        self.path.as_ref() == path && (!shared || self.untracked == untracked)
    }

    /// What `summary` says of it, if it lists it, as [`Self::is`] finds it.
    pub(super) fn in_summary<'a>(&self, summary: &'a ChangeSummary) -> Option<&'a ChangedFile> {
        let shared = summary.files.iter().filter(|file| file.path == self.path.as_ref()).count() > 1;
        summary.files.iter().find(|file| self.is(&file.path, file.at().untracked, shared))
    }

    /// It as a summary names it, and one file's diff asks for it.
    pub(super) fn at(&self) -> ChangedFileAt {
        ChangedFileAt { untracked: self.untracked, path: self.path.to_string() }
    }
}

/// One changed file as Changes lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangedRow {
    pub path: SharedString,
    /// It's the untracked file at its path, as [`ChangedEntry`] says.
    pub untracked: bool,
    pub status: FileStatus,
    pub added: usize,
    pub removed: usize,
}

/// The files Changes lists, by path, and how many it leaves out.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub rows: Arc<[ChangedRow]>,
    /// How many the workspace changes past those in `rows`, which only a summary leaves out.
    pub left_out: u64,
    /// The whole diff left out some untracked files, and doesn't say how many. A summary counts every file it leaves
    /// out in `left_out`, so this is never so of one.
    pub truncated: bool,
}

impl ChangedRow {
    /// Which changed file it is.
    pub fn entry(&self) -> ChangedEntry {
        ChangedEntry { path: self.path.clone(), untracked: self.untracked }
    }
}

impl Listing {
    /// How many files the workspace changes, those left out included.
    pub fn changed(&self) -> usize {
        self.rows.len().saturating_add(counted(self.left_out))
    }
}

impl Review {
    /// Takes what client-core holds of the workspace's changed files, `None` where the hub doesn't send them, and lists
    /// them while they're current, in place of what was listed before. Otherwise the whole diff's files are listed, as
    /// from a hub that doesn't send them: a summary as of an earlier read can be older than the whole diff, which is
    /// read again as the tab is switched to. A summary that was current and isn't now, as when its worker goes, or the
    /// connection does, stays listed until the whole diff is next read, at the moments it's read.
    pub fn take_summary(&mut self, changes: Option<&Observed<ChangeSummary>>, cx: &mut Context<Self>) {
        let Some(Observed::Current { value: summary }) = changes else {
            if let Some(Listed::Summary(listed)) = &mut self.listed {
                listed.current = false;
            }
            return;
        };
        let (file, seen) = match self.listed.take() {
            // Of the same worker, since one that goes takes the summary's currency with it, so what it refused it
            // refuses still.
            Some(Listed::Summary(Summary { current: true, file, .. })) => (file, None),
            // The worker may be another now, so what was refused is asked for again.
            Some(Listed::Summary(mut listed @ Summary { current: false, .. })) => {
                listed.ask_again();
                (listed.file, None)
            }
            Some(Listed::WholeDiff(whole)) => {
                (SummaryFile::OneFile(None), self.diffed.as_ref().map(|entry| whole.taken(entry)))
            }
            None => (SummaryFile::OneFile(None), None),
        };
        self.listed = Some(Listed::Summary(Summary { summarized: summary.clone(), current: true, file }));
        self.list_again(cx);
        self.say_what_changes(cx);
        // The file the whole diff showed shows until it's read on its own.
        match seen {
            Some(seen) => self.read_one_file(Some(seen), cx),
            None => self.read_the_file_again_if_it_moved(cx),
        }
    }

    /// What Changes lists now, or `None` until the changes are first read.
    pub fn listing(&self) -> Option<&Listing> {
        self.listing.as_ref()
    }

    /// How many lines the workspace's changes add and remove, in all: the summary's totals, over every file, or the
    /// sums of the whole diff's files.
    pub fn totals(&self) -> (usize, usize) {
        match &self.listed {
            Some(Listed::Summary(Summary { summarized, .. })) => {
                (counted(summarized.lines.added), counted(summarized.lines.removed))
            }
            Some(Listed::WholeDiff(whole)) => whole
                .files
                .iter()
                .fold((0, 0), |(added, removed), file| (added + file.added(), removed + file.removed())),
            None => (0, 0),
        }
    }

    /// Lists again what Changes shows, from what's listed.
    pub(super) fn list_again(&mut self, cx: &mut Context<Self>) {
        self.listing = self.listed.as_ref().map(|listed| match listed {
            Listed::Summary(Summary { summarized, .. }) => Listing {
                rows: by_path(summarized.files.iter().map(summarized_row).collect()),
                left_out: summarized.left_out,
                truncated: false,
            },
            Listed::WholeDiff(whole) => Listing {
                rows: by_path(whole.entries().map(|(untracked, file)| patched_row(file, untracked)).collect()),
                left_out: 0,
                truncated: whole.truncated,
            },
        });
        cx.notify();
    }
}

/// How a file of the summary changed, as the whole diff would say it: one that isn't tracked as added, and one that
/// changed between a file, a link and a submodule as modified.
pub(super) fn file_status(status: ChangeStatus) -> FileStatus {
    match status {
        ChangeStatus::Added | ChangeStatus::Untracked => FileStatus::Added,
        ChangeStatus::Deleted => FileStatus::Removed,
        ChangeStatus::Renamed => FileStatus::Renamed,
        ChangeStatus::Modified | ChangeStatus::TypeChanged | ChangeStatus::Unknown => FileStatus::Modified,
    }
}

/// `rows` in order of their paths, as Changes lists them, the tracked file at a path before the untracked one.
fn by_path(mut rows: Vec<ChangedRow>) -> Arc<[ChangedRow]> {
    rows.sort_by(|a, b| (&a.path, a.untracked).cmp(&(&b.path, b.untracked)));
    rows.into()
}

/// A file as the summary has it, with the status the whole diff would give it.
fn summarized_row(file: &ChangedFile) -> ChangedRow {
    let status = file_status(file.status);
    let count = |lines: fn(LineCounts) -> u64| file.lines.map_or(0, |counts| counted(lines(counts)));
    ChangedRow {
        path: file.path.clone().into(),
        untracked: file.at().untracked,
        status,
        added: count(|counts| counts.added),
        removed: count(|counts| counts.removed),
    }
}

/// A file as the whole diff has it, the untracked one at its path where `untracked` says so.
fn patched_row(file: &FilePatch, untracked: bool) -> ChangedRow {
    let (path, status) = (file.path().clone(), file.status());
    ChangedRow { path, untracked, status, added: file.added(), removed: file.removed() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summarized_file_lists_with_the_status_the_whole_diff_would_give_it() {
        let file = |status| ChangedFile {
            path: "a.rs".into(),
            from: None,
            status,
            lines: Some(LineCounts { added: 2, removed: 1 }),
            version: None,
            name_not_utf8: false,
        };
        let cases = [
            (ChangeStatus::Added, FileStatus::Added),
            (ChangeStatus::Untracked, FileStatus::Added),
            (ChangeStatus::Modified, FileStatus::Modified),
            (ChangeStatus::TypeChanged, FileStatus::Modified),
            (ChangeStatus::Deleted, FileStatus::Removed),
            (ChangeStatus::Renamed, FileStatus::Renamed),
            (ChangeStatus::Unknown, FileStatus::Modified),
        ];

        for (status, listed) in cases {
            let row = summarized_row(&file(status));
            assert_eq!((row.status, row.added, row.removed), (listed, 2, 1), "{status:?}");
        }
    }
}
