//! What Changes lists, which is the hub's summary of the changed files, and the file the file tab shows, read on its
//! own with it, so every reader asks one value.

use protocol::model::{ChangeSummary, CommitId};

use super::Review;
use super::listing::ChangedEntry;
use super::one_file::OneFile;
use crate::patch::FilePatch;

/// What Changes lists, once the workspace's level has listed it.
#[derive(Debug)]
pub(super) struct Summary {
    /// What it lists, as the hub sent it, whether current or as of an earlier read. `None` while the hub's summary isn't
    /// known, which lists nothing, as a workspace that changes nothing does. The files it leaves out aren't read, so
    /// only how many there are shows.
    pub(super) summarized: Option<ChangeSummary>,
    /// The file the file tab shows, read with one file's diff. `None` while the tab shows no file.
    pub(super) file: Option<OneFile>,
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
    /// Its name isn't UTF-8, as the summary marks it, so the name the summary gives it, with those bytes replaced,
    /// names no file, and nothing is read for it.
    NameNotUtf8,
}

impl Review {
    /// What the file tab has of the diff of the file it shows, as one file's diff read it, and nothing of a file the
    /// summary marks as named in bytes that aren't UTF-8. One that hasn't been read couldn't be while the worktree
    /// can't be read, since nothing is.
    pub fn diffed(&self) -> Diffed<'_> {
        let unread = if self.worktree_readable { Diffed::Loading } else { Diffed::Failed };
        let Some(entry) = &self.diffed else { return Diffed::Loading };
        if self.names_no_file(entry) {
            return Diffed::NameNotUtf8;
        }
        match self.listed.as_ref().and_then(|listed| listed.file.as_ref()) {
            Some(file) if file.of(entry) => file.diffed(),
            Some(_) | None => unread,
        }
    }

    /// Whether the summary listed marks `entry` as named in bytes that aren't UTF-8, so the name it gives names no file
    /// a request could read.
    pub(super) fn names_no_file(&self, entry: &ChangedEntry) -> bool {
        let Some(summarized) = self.listed.as_ref().and_then(|listed| listed.summarized.as_ref()) else { return false };
        entry.in_summary(summarized).is_some_and(|file| file.name_not_utf8)
    }

    /// The patch of the file the file tab shows, as [`Self::diffed`] has it. `None` once it no longer changes anything,
    /// and until it's read.
    pub fn file(&self) -> Option<&FilePatch> {
        match self.diffed() {
            Diffed::Patch(found) => Some(found),
            Diffed::Loading | Diffed::Unchanged | Diffed::Failed | Diffed::NameNotUtf8 => None,
        }
    }

    /// The patch of the file the file tab shows, as [`Self::file`] has it, with the commit it was diffed against.
    pub fn file_against(&self) -> Option<(&FilePatch, &CommitId)> {
        let file = self.file()?;
        Some((file, self.listed.as_ref()?.file.as_ref()?.base_commit()?))
    }

    /// The commit the file the file tab shows was diffed against, which holds it as it was before the change, and what
    /// the diff leaves out of it is read from: the one file's diff's, or else the summary's. `None` until the changes
    /// are listed.
    pub fn base_commit(&self) -> Option<&CommitId> {
        let listed = self.listed.as_ref()?;
        let read = listed.file.as_ref().and_then(OneFile::base_commit);
        read.or(listed.summarized.as_ref().map(|summarized| &summarized.base_commit))
    }
}
