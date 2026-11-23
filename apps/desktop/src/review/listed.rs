//! What Changes lists, which is either the whole diff or the hub's summary of the changed files, and where the file the
//! file tab shows is read from with each, so every reader asks one value which of them it is.

use protocol::model::CommitId;

use super::Review;
use super::listing::{ChangedEntry, Summarized};
use super::one_file::{OneFile, Taken};
use crate::patch::FilePatch;

/// What Changes lists.
#[derive(Debug)]
#[expect(clippy::large_enum_variant, reason = "a workspace's pane holds one, so boxing the larger saves nothing")]
pub(super) enum Listed {
    /// The whole diff, as last read, which lists every file and holds each one's patch.
    WholeDiff(WholeDiff),
    /// The hub's summary of the changed files, with the pages read past it.
    Summary(Summary),
}

/// The whole diff, as one read of it found it.
#[derive(Debug)]
pub(super) struct WholeDiff {
    /// Every changed file, in order of path.
    pub(super) files: Vec<FilePatch>,
    /// The commit the diff was taken from, which holds each file as it was before the change. `None` from a worker
    /// too old to say.
    pub(super) base_commit: Option<CommitId>,
    /// The hub left some untracked files out.
    pub(super) truncated: bool,
}

/// A summary the hub sent, while Changes lists it.
#[derive(Debug)]
pub(super) struct Summary {
    pub(super) summarized: Summarized,
    /// Whether the hub keeps it current now. One that stops being current, as when its worker goes, or the
    /// connection does, stays listed until the whole diff is next read, at the moments it's read, with the file read
    /// with it.
    pub(super) current: bool,
    pub(super) file: SummaryFile,
}

/// Where the file the file tab shows is read from while a summary is listed.
#[derive(Debug)]
#[expect(clippy::large_enum_variant, reason = "a workspace's pane holds one, so boxing the larger saves nothing")]
pub(super) enum SummaryFile {
    /// On its own, with one file's diff. `None` while the tab shows no file.
    OneFile(Option<OneFile>),
    /// From the whole diff, `None` until it's read, since the hub, or its worker, refused one file's diff. Until the
    /// next connection, or until the summary stops being current, as it does when its worker goes, since the hub
    /// replaces an older worker with one of its own build.
    WholeDiff(Option<WholeDiff>),
}

/// What the file tab has of one file's diff.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Diffed<'a> {
    /// It hasn't been read.
    Loading,
    /// Its patch: as read last, which shows while it's read again.
    Patch(&'a FilePatch),
    /// The worktree changes nothing at its path now.
    Unchanged,
    /// It couldn't be read, nor had it been before. It's read again with the next summary, or at the next moment that
    /// reads the changes again, as on a new connection.
    Failed,
}

impl Summary {
    /// What the hub, or its worker, refused is asked for again, as from another.
    pub(super) fn ask_again(&mut self) {
        if let SummaryFile::WholeDiff(_) = self.file {
            self.file = SummaryFile::OneFile(None);
        }
        self.summarized.ask_again();
    }
}

impl WholeDiff {
    /// Each file, and whether it's the untracked one at its path. The whole diff is git's diff of what it tracks, and
    /// then each untracked file's, so of two at one path, which are in that order, the second is the untracked one.
    pub(super) fn entries(&self) -> impl Iterator<Item = (bool, &FilePatch)> {
        let before = std::iter::once(None).chain(self.files.iter().map(Some));
        before.zip(&self.files).map(|(before, file)| (before.is_some_and(|before| before.path() == file.path()), file))
    }

    /// The patch of `entry`, if it has one, as [`ChangedEntry::is`] finds it.
    fn patch(&self, entry: &ChangedEntry) -> Option<&FilePatch> {
        let shared = self.entries().filter(|(_, file)| *file.path() == entry.path).count() > 1;
        let mut entries = self.entries();
        entries.find(|(untracked, file)| entry.is(file.path(), *untracked, shared)).map(|(_, file)| file)
    }

    /// What it has of `entry`.
    fn diffed(&self, entry: &ChangedEntry) -> Diffed<'_> {
        self.patch(entry).map_or(Diffed::Unchanged, Diffed::Patch)
    }

    /// `entry` as it found it, to show while that file is read on its own.
    pub(super) fn taken(&self, entry: &ChangedEntry) -> Taken {
        let base = self.base_commit.clone();
        self.patch(entry).map_or(Taken::Unchanged, |patch| Taken::Patch { patch: patch.clone(), base })
    }
}

impl Review {
    /// What the file tab has of the diff of the file it shows: from the whole diff, where Changes lists it or the hub
    /// refused one file's diff, and otherwise as one file's diff read it.
    pub fn diffed(&self) -> Diffed<'_> {
        let Some(entry) = &self.diffed else { return Diffed::Loading };
        match &self.listed {
            None => Diffed::Loading,
            Some(Listed::WholeDiff(whole)) => whole.diffed(entry),
            Some(Listed::Summary(summary)) => match &summary.file {
                SummaryFile::WholeDiff(Some(whole)) => whole.diffed(entry),
                SummaryFile::OneFile(Some(file)) if file.of(entry) => file.diffed(),
                SummaryFile::WholeDiff(None) | SummaryFile::OneFile(_) => Diffed::Loading,
            },
        }
    }

    /// The patch of the file the file tab shows, as [`Self::diffed`] has it. `None` once it no longer changes anything,
    /// and until it's read.
    pub fn file(&self) -> Option<&FilePatch> {
        match self.diffed() {
            Diffed::Patch(found) => Some(found),
            Diffed::Loading | Diffed::Unchanged | Diffed::Failed => None,
        }
    }

    /// The commit the file the file tab shows was diffed against, which holds it as it was before the change, and what
    /// the diff leaves out of it is read from: the whole diff's, or the one file's diff's, or else the summary's.
    /// `None` until the changes are read, or from a worker too old to say.
    pub fn base_commit(&self) -> Option<&CommitId> {
        match &self.listed {
            None => None,
            Some(Listed::WholeDiff(whole)) => whole.base_commit.as_ref(),
            Some(Listed::Summary(summary)) => {
                let read = match &summary.file {
                    SummaryFile::WholeDiff(whole) => whole.as_ref().and_then(|whole| whole.base_commit.as_ref()),
                    SummaryFile::OneFile(file) => file.as_ref().and_then(OneFile::base_commit),
                };
                read.or(Some(&summary.summarized.base_commit))
            }
        }
    }
}
