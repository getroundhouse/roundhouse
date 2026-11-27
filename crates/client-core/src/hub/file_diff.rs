//! Reading every page of one file's diff, as one patch of one version of the file.

use protocol::model::{ChangedFileAt, FileDiff, FileDiffBody, FileVersion, WorkspaceId};

use super::Hub;
use crate::Result;

/// How many times a file's pages are read from its start again because a later page wasn't of the version the first
/// was, after which it's taken as changing as it's read, and read again when the summary next moves it, as it will for
/// a file that's being written.
const MOST_READS_AGAIN: usize = 3;

impl Hub {
    /// Every page of the diff of the file `at` names, renamed from `from`, as one, read from its start again when a
    /// later page isn't of the version of the file the first is, up to [`MOST_READS_AGAIN`] times. A later page is of
    /// it when it names the same version and is a patch, rather than binary or unchanged. Two pages that name no
    /// version are of one only where `listed`, the version the summary names, is none too, as for a deleted file,
    /// since otherwise naming none says the file changed while that page was read. `None` when its pages weren't of
    /// one version each time.
    ///
    /// Every page is read on one connection, the one that had caught up as this started, as [`Hub::call`] waits, and
    /// without one it fails at once, as [`Hub::ask`] does.
    pub fn read_file_diff(
        &self,
        id: &WorkspaceId,
        at: &ChangedFileAt,
        from: Option<&str>,
        listed: Option<&FileVersion>,
    ) -> Result<Option<FileDiff>> {
        let connection = self.ready()?;
        'read: for _ in 0..=MOST_READS_AGAIN {
            let first = connection.workspace_file_diff(id, at, from, 0)?;
            let FileDiffBody::Patch { patch: mut whole, mut next_line } = first.body else { return Ok(Some(first)) };
            let known = first.version.is_some() || listed.is_none();
            while let Some(line) = next_line {
                let page = connection.workspace_file_diff(id, at, from, line)?;
                let FileDiffBody::Patch { patch, next_line: next } = page.body else { continue 'read };
                if !known || page.version != first.version {
                    continue 'read;
                }
                whole.push_str(&patch);
                next_line = next;
            }
            let body = FileDiffBody::Patch { patch: whole, next_line: None };
            return Ok(Some(FileDiff { body, ..first }));
        }
        Ok(None)
    }
}
