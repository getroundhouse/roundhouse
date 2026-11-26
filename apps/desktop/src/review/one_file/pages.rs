//! Reading every page of one file's diff as one patch of one version of the file, and taking in what it says.

use client_core::{ClientError, Hub};
use gpui_kit::SharedString;
use protocol::client::workspaces::WorkspaceFileDiff;
use protocol::model::{ChangeStatus, ChangedFileAt, FileDiff, FileDiffBody, FileVersion, WorkspaceId};

use super::file::Taken;
use crate::patch::{self, FilePatch};
use crate::review::listing::file_status;

/// How many times a file's pages are read from its start again because a later page wasn't of the version the first
/// was, after which it's taken as changing as it's read, and read again when the summary next moves it, as it will for
/// a file that's being written.
const MOST_READS_AGAIN: usize = 3;

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

/// Every page of the diff of the file `at` names, renamed from `from`, as one, read from its start again when a later
/// page isn't of the version of the file the first is, up to [`MOST_READS_AGAIN`] times. A later page is of it when it
/// names the same version and is a patch, rather than binary or unchanged. Two pages that name no version are of one
/// only where `listed`, the version the summary names, is none too, as for a deleted file, since otherwise naming none
/// says the file changed while that page was read.
pub(super) fn every_page(
    hub: &Hub,
    id: &WorkspaceId,
    at: &ChangedFileAt,
    from: Option<&str>,
    listed: Option<&FileVersion>,
) -> Result<FileDiff, Unread> {
    'read: for _ in 0..=MOST_READS_AGAIN {
        let first = hub.call(page_of(id, at, from, 0)).map_err(Unread::Hub)?;
        let FileDiffBody::Patch { patch: mut whole, mut next_line } = first.body else { return Ok(first) };
        let known = first.version.is_some() || listed.is_none();
        while let Some(line) = next_line {
            let page = hub.call(page_of(id, at, from, line)).map_err(Unread::Hub)?;
            let FileDiffBody::Patch { patch, next_line: next } = page.body else { continue 'read };
            if !known || page.version != first.version {
                continue 'read;
            }
            whole.push_str(&patch);
            next_line = next;
        }
        let body = FileDiffBody::Patch { patch: whole, next_line: None };
        return Ok(FileDiff { body, ..first });
    }
    Err(Unread::KeptChanging)
}

/// What asks for the page of the diff of the file `at` names, renamed from `from`, from the patch's line `first_line`.
fn page_of(id: &WorkspaceId, at: &ChangedFileAt, from: Option<&str>, first_line: u32) -> WorkspaceFileDiff {
    let ChangedFileAt { untracked, path } = at.clone();
    WorkspaceFileDiff { id: id.clone(), path, untracked, from: from.map(str::to_string), first_line }
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
