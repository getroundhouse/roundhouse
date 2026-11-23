//! The file the file tab shows, and where its newest read is.

use gpui_kit::SharedString;
use protocol::model::{ChangeStatus, CommitId, FileVersion};

use crate::patch::FilePatch;
use crate::review::listed::Diffed;

/// What the summary says of a file, which one file's diff of it is read again only when it changes.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Key {
    pub(super) base: CommitId,
    pub(super) status: ChangeStatus,
    pub(super) from: Option<String>,
    pub(super) version: Option<FileVersion>,
}

/// The file the file tab shows, as one file's diff reads it.
#[derive(Debug)]
pub(in crate::review) struct OneFile {
    pub(super) path: SharedString,
    /// What the summary said of it when it was last asked for, or `None` where it lists it nowhere.
    pub(super) key: Option<Key>,
    pub(super) read: Read,
}

/// Where the newest read of a file is.
#[derive(Debug)]
pub(super) enum Read {
    /// Being read, with what was read of it before shown meanwhile, if anything was.
    Reading {
        previous: Option<Taken>,
    },
    Taken(Taken),
    /// It couldn't be read, so it's read again with the next summary, or at the next moment that reads the changes
    /// again, as on a new connection, with what was read of it before shown meanwhile, if anything was.
    Failed {
        previous: Option<Taken>,
    },
}

/// What a read found of a file.
#[derive(Clone, Debug)]
pub(in crate::review) enum Taken {
    /// Its patch, against `base`, the commit what the patch leaves out is read from. `None` from a worker too old to
    /// name it.
    Patch { patch: FilePatch, base: Option<CommitId> },
    /// The worktree changes nothing at its path now.
    Unchanged,
}

impl OneFile {
    pub(in crate::review) fn of(&self, path: &str) -> bool {
        self.path.as_ref() == path
    }

    /// What the file tab shows of it.
    pub(in crate::review) fn diffed(&self) -> Diffed<'_> {
        match &self.read {
            Read::Taken(taken) | Read::Reading { previous: Some(taken) } | Read::Failed { previous: Some(taken) } => {
                taken.diffed()
            }
            Read::Reading { previous: None } => Diffed::Loading,
            Read::Failed { previous: None } => Diffed::Failed,
        }
    }

    /// The commit the patch shown is against.
    pub(in crate::review) fn base_commit(&self) -> Option<&CommitId> {
        match self.shown()? {
            Taken::Patch { base, .. } => base.as_ref(),
            Taken::Unchanged => None,
        }
    }

    /// What was read of it last, which shows.
    pub(super) fn shown(&self) -> Option<&Taken> {
        match &self.read {
            Read::Taken(taken) => Some(taken),
            Read::Reading { previous } | Read::Failed { previous } => previous.as_ref(),
        }
    }
}

impl Taken {
    fn diffed(&self) -> Diffed<'_> {
        match self {
            Taken::Patch { patch, .. } => Diffed::Patch(patch),
            Taken::Unchanged => Diffed::Unchanged,
        }
    }
}
