//! What Changes lists of a workspace's changed files: the hub's summary of them, with the pages past it read so far,
//! where the hub sends one, and otherwise the files of the whole diff.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, SharedString};
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, CommitId, LineCounts, Observed};

use super::listed::{Listed, Summary, SummaryFile};
use super::{Review, ReviewEvent};
use crate::patch::{FilePatch, FileStatus};
use client_core::workspace_lines::counted;

/// One changed file as Changes lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangedRow {
    pub path: SharedString,
    pub status: FileStatus,
    pub added: usize,
    pub removed: usize,
}

/// What the hub's summary of the workspace's changes lists, and the pages read past it, until the summary changes.
#[derive(Debug)]
pub(super) struct Summarized {
    /// The commit the summary is taken against.
    pub(super) base_commit: CommitId,
    /// The summary's files, then each page's, in the order the hub lists them, which a page goes on from.
    files: Vec<ChangedFile>,
    /// Over every file the workspace changes, whether listed or not.
    lines: LineCounts,
    /// How many files come after the last listed.
    left_out: u64,
    pub(super) paging: Paging,
}

/// Whether the files past those listed are being read, or can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paging {
    /// Not being read now.
    Idle,
    /// The next page is being read.
    Reading,
    /// The hub, or its worker, is from before pages, so only how many there are shows.
    Refused,
}

/// The files Changes lists, by path, and how many it leaves out.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub rows: Arc<[ChangedRow]>,
    /// How many the workspace changes past those in `rows`, which only a summary leaves out.
    pub left_out: u64,
    /// Whether they can be read, as [`Paging`] says, where any are left out.
    pub paging: Option<Paging>,
    /// The whole diff left out some untracked files, and doesn't say how many. A summary counts every file it leaves
    /// out in `left_out`, so this is never so of one.
    pub truncated: bool,
}

impl Listing {
    /// How many files the workspace changes, those left out included.
    pub fn changed(&self) -> usize {
        self.rows.len().saturating_add(counted(self.left_out))
    }
}

impl Review {
    /// Takes what client-core holds of the workspace's changed files, `None` where the hub doesn't send them, and lists
    /// them while they're current, letting go of the pages read past the summary before. Otherwise the whole diff's
    /// files are listed, as from a hub that doesn't send them: a summary as of an earlier read can be older than the
    /// whole diff, which is read again as the tab is switched to. A summary that was current and isn't now, as when its
    /// worker goes, or the connection does, stays listed until the whole diff is next read, at the moments it's read.
    pub fn take_summary(&mut self, changes: Option<&Observed<ChangeSummary>>, cx: &mut Context<Self>) {
        let Some(Observed::Current { value: summary }) = changes else {
            if let Some(Listed::Summary(listed)) = &mut self.listed {
                listed.current = false;
            }
            return;
        };
        self.summaries_taken += 1;
        let (paging, file, seen) = match self.listed.take() {
            // Of the same worker, since one that goes takes the summary's currency with it, so what it refused it
            // refuses still.
            Some(Listed::Summary(Summary { current: true, summarized, file })) => (summarized.paging, file, None),
            // The worker may be another now, so what was refused is asked for again.
            Some(Listed::Summary(mut listed @ Summary { current: false, .. })) => {
                listed.ask_again();
                (listed.summarized.paging, listed.file, None)
            }
            Some(Listed::WholeDiff(whole)) => {
                (Paging::Idle, SummaryFile::OneFile(None), self.diffed.as_ref().map(|path| whole.taken(path)))
            }
            None => (Paging::Idle, SummaryFile::OneFile(None), None),
        };
        let summarized = Summarized {
            base_commit: summary.base_commit.clone(),
            files: summary.files.clone(),
            lines: summary.lines,
            left_out: summary.left_out,
            // A page being read is of the summary before, so it isn't listed with this one.
            paging: if paging == Paging::Refused { Paging::Refused } else { Paging::Idle },
        };
        self.listed = Some(Listed::Summary(Summary { summarized, current: true, file }));
        self.list_again(cx);
        self.say_what_changes(cx);
        // The file the whole diff showed shows until it's read on its own.
        match seen {
            Some(seen) => self.read_one_file(Some(seen), cx),
            None => self.read_the_file_again_if_it_moved(cx),
        }
    }

    /// Reads the page of files past those listed, as the hub's summary lists them, unless one is being read, and lists
    /// it once it comes, unless the summary has changed since, or the page is against another base than the summary,
    /// which it then doesn't follow.
    pub fn read_more(&mut self, cx: &mut Context<Self>) {
        let Some(Listed::Summary(Summary { summarized, .. })) = &mut self.listed else { return };
        if summarized.left_out == 0 || summarized.paging != Paging::Idle {
            return;
        }
        let Some(after) = summarized.files.last().map(ChangedFile::at) else { return };
        summarized.paging = Paging::Reading;
        let (asked, client, id) = (self.summaries_taken, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let read = cx.background_spawn(async move { client.workspace_changed_files(&id, after) }).await;
            let _ = this.update(cx, |t, cx| {
                let Some(Listed::Summary(Summary { summarized, .. })) = &mut t.listed else { return };
                if t.summaries_taken != asked {
                    return;
                }
                match read {
                    Ok(page) if page.base_commit == summarized.base_commit => {
                        summarized.files.extend(page.files);
                        summarized.left_out = page.left_out;
                        summarized.paging = Paging::Idle;
                    }
                    // Of the worktree after its base moved, which the next summary lists.
                    Ok(_) => summarized.paging = Paging::Idle,
                    Err(e) if e.is_unsupported() => summarized.paging = Paging::Refused,
                    Err(e) => {
                        summarized.paging = Paging::Idle;
                        cx.emit(ReviewEvent::Error(client_core::refusals::couldnt("read more changed files", &e)));
                    }
                }
                t.list_again(cx);
            });
        })
        .detach();
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
                paging: (summarized.left_out > 0).then_some(summarized.paging),
                truncated: false,
            },
            Listed::WholeDiff(whole) => Listing {
                rows: by_path(whole.files.iter().map(patched_row).collect()),
                left_out: 0,
                paging: None,
                truncated: whole.truncated,
            },
        });
        cx.notify();
    }
}

impl Summarized {
    /// A hub that refused a page is asked for the next, as from another.
    pub(super) fn ask_again(&mut self) {
        if self.paging == Paging::Refused {
            self.paging = Paging::Idle;
        }
    }

    /// What the summary, or a page read past it, says of the file at `path`, if it lists it.
    pub(super) fn file(&self, path: &str) -> Option<&ChangedFile> {
        self.files.iter().find(|file| file.path == path)
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

/// `rows` in order of their paths, as Changes lists them.
fn by_path(mut rows: Vec<ChangedRow>) -> Arc<[ChangedRow]> {
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows.into()
}

/// A file as the summary has it, with the status the whole diff would give it.
fn summarized_row(file: &ChangedFile) -> ChangedRow {
    let status = file_status(file.status);
    let count = |lines: fn(LineCounts) -> u64| file.lines.map_or(0, |counts| counted(lines(counts)));
    ChangedRow {
        path: file.path.clone().into(),
        status,
        added: count(|counts| counts.added),
        removed: count(|counts| counts.removed),
    }
}

/// A file as the whole diff has it.
fn patched_row(file: &FilePatch) -> ChangedRow {
    ChangedRow { path: file.path().clone(), status: file.status(), added: file.added(), removed: file.removed() }
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
