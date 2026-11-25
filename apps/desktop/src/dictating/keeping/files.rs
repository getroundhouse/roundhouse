//! Writing a file in the recordings' folder.

use std::io::{self, Write as _};
use std::path::Path;

/// Replaces the file at `file` with `written` all at once, so the app going as it's written leaves the last one whole.
pub(super) fn replace(file: &Path, written: &[u8]) -> io::Result<()> {
    let partial = file.with_extension("json.partial");
    let mut open = private_files::file_options().create(true).truncate(true).write(true).open(&partial)?;
    open.write_all(written)?;
    std::fs::rename(&partial, file)
}

pub(super) fn gone_already(error: io::Error) -> io::Result<()> {
    if error.kind() == io::ErrorKind::NotFound { Ok(()) } else { Err(error) }
}
