//! What Changes lists of a workspace's changed files: the hub's summary of them, with the pages past it read so far,
//! where the hub sends one, and otherwise the files of the whole diff.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, SharedString};
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, LineCounts, Observed};

use super::{Review, ReviewEvent};
use crate::patch::{FilePatch, FileStatus};

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
}

impl Listing {
    /// How many files the workspace changes, those left out included.
    pub fn changed(&self) -> usize {
        self.rows.len().saturating_add(usize::try_from(self.left_out).unwrap_or(usize::MAX))
    }
}

impl Review {
    /// Takes what client-core holds of the workspace's changed files, `None` where the hub doesn't send them, and lists
    /// them while they're current, letting go of the pages read past the summary before. Otherwise the whole diff's
    /// files are listed, as from a hub that doesn't send them: a summary as of an earlier read can be older than the
    /// whole diff, which is read again as the tab is switched to.
    pub fn take_summary(&mut self, changes: Option<&Observed<ChangeSummary>>, cx: &mut Context<Self>) {
        let summary = match changes {
            Some(Observed::Current { value }) => Some(value),
            Some(Observed::AsOf { .. } | Observed::NotKnown | Observed::Unknown) | None => None,
        };
        let paging = self.summarized.as_ref().map(|summarized| summarized.paging);
        self.summarized = summary.map(|summary| Summarized {
            files: summary.files.clone(),
            lines: summary.lines,
            left_out: summary.left_out,
            // A hub that refused a page refuses the next.
            paging: if paging == Some(Paging::Refused) { Paging::Refused } else { Paging::Idle },
        });
        self.pages_asked += 1;
        self.list_again(cx);
    }

    /// Reads the page of files past those listed, as the hub's summary lists them, unless one is being read, and lists
    /// it once it comes, unless the summary has changed since.
    pub fn read_more(&mut self, cx: &mut Context<Self>) {
        let Some(summarized) = self.summarized.as_mut().filter(|s| s.left_out > 0 && s.paging == Paging::Idle) else {
            return;
        };
        let Some(after) = summarized.files.last().map(ChangedFile::at) else { return };
        summarized.paging = Paging::Reading;
        let (asked, client, id) = (self.pages_asked, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let read = cx.background_spawn(async move { client.workspace_changed_files(&id, after) }).await;
            let _ = this.update(cx, |t, cx| {
                let Some(summarized) = t.summarized.as_mut().filter(|_| t.pages_asked == asked) else { return };
                match read {
                    Ok(page) => {
                        summarized.files.extend(page.files);
                        summarized.left_out = page.left_out;
                        summarized.paging = Paging::Idle;
                    }
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
        if let Some(summarized) = &self.summarized {
            let count = |lines: u64| usize::try_from(lines).unwrap_or(usize::MAX);
            return (count(summarized.lines.added), count(summarized.lines.removed));
        }
        let files = self.files.iter().flatten();
        files.fold((0, 0), |(added, removed), file| (added + file.added(), removed + file.removed()))
    }

    /// Lists again what Changes shows, from the summary where there is one, and otherwise from the whole diff.
    pub(super) fn list_again(&mut self, cx: &mut Context<Self>) {
        self.listing = match (&self.summarized, &self.files) {
            (Some(summarized), _) => Some(Listing {
                rows: by_path(summarized.files.iter().map(summarized_row).collect()),
                left_out: summarized.left_out,
                paging: (summarized.left_out > 0).then_some(summarized.paging),
            }),
            (None, Some(files)) => {
                Some(Listing { rows: by_path(files.iter().map(patched_row).collect()), left_out: 0, paging: None })
            }
            (None, None) => None,
        };
        cx.notify();
    }
}

/// `rows` in order of their paths, as Changes lists them.
fn by_path(mut rows: Vec<ChangedRow>) -> Arc<[ChangedRow]> {
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows.into()
}

/// A file as the summary has it. One that isn't tracked lists as added, and one that changed between a file, a link
/// and a submodule as modified, as the whole diff shows them.
fn summarized_row(file: &ChangedFile) -> ChangedRow {
    let status = match file.status {
        ChangeStatus::Added | ChangeStatus::Untracked => FileStatus::Added,
        ChangeStatus::Deleted => FileStatus::Removed,
        ChangeStatus::Renamed => FileStatus::Renamed,
        ChangeStatus::Modified | ChangeStatus::TypeChanged | ChangeStatus::Unknown => FileStatus::Modified,
    };
    let count = |lines: fn(LineCounts) -> u64| {
        file.lines.map_or(0, |counts| usize::try_from(lines(counts)).unwrap_or(usize::MAX))
    };
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
