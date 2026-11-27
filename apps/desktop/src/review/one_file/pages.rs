//! Reading every page of one file's diff as one patch of one version of the file, and taking in what it says.

use client_core::{ClientError, Hub};
use gpui_kit::SharedString;
use protocol::model::{ChangeStatus, ChangedFileAt, FileDiff, FileDiffBody, FileVersion, WorkspaceId};

use super::file::Taken;
use crate::patch::{self, FilePatch};
use crate::review::listing::file_status;

/// Why a file's diff wasn't taken.
#[derive(Debug)]
pub(super) enum Unread {
    /// The hub refused it, or couldn't read it.
    Hub(ClientError),
    /// Its pages weren't of one version of it, each time it was read.
    KeptChanging,
    /// Its patch doesn't parse.
    Unparsed(String),
    /// It's of a form a newer peer sends, which this build can't draw.
    Unknown,
}

impl Unread {
    /// What the person is told of it, if anything. Nothing of a file that changed as it was read, which the summary
    /// moves again, so it's read again, nor of a form this build can't draw, which the file tab says it couldn't read.
    pub(super) fn said(&self) -> Option<String> {
        match self {
            Unread::Hub(e) => Some(client_core::refusals::couldnt("load the file's diff", e)),
            Unread::Unparsed(e) => Some(format!("Couldn't read the file's diff: {e}")),
            Unread::KeptChanging | Unread::Unknown => None,
        }
    }
}

/// Every page of the diff of the file `at` names, renamed from `from`, as one, as [`Hub::read_file_diff`] reads it.
pub(super) fn every_page(
    hub: &Hub,
    id: &WorkspaceId,
    at: &ChangedFileAt,
    from: Option<&str>,
    listed: Option<&FileVersion>,
) -> Result<FileDiff, Unread> {
    hub.read_file_diff(id, at, from, listed).map_err(Unread::Hub)?.ok_or(Unread::KeptChanging)
}

/// What `diff` says of the file at `path`, as the file tab draws it, moved from `from` and changed as `status` says,
/// where the summary lists it.
pub(super) fn taken(
    path: &SharedString,
    from: Option<String>,
    status: Option<ChangeStatus>,
    diff: FileDiff,
) -> Result<Taken, Unread> {
    let base = diff.base_commit;
    match diff.body {
        FileDiffBody::Patch { patch, .. } => {
            let files = patch::parse(&patch).map_err(|e| Unread::Unparsed(e.to_string()))?;
            Ok(files.into_iter().next().map_or(Taken::Unchanged, |patch| Taken::Patch { patch, base }))
        }
        FileDiffBody::Binary => {
            let status = file_status(status.unwrap_or(ChangeStatus::Modified));
            Ok(Taken::Patch { patch: FilePatch::binary(path.clone(), from.map(SharedString::from), status), base })
        }
        FileDiffBody::Unchanged => Ok(Taken::Unchanged),
        FileDiffBody::Unknown => Err(Unread::Unknown),
    }
}
