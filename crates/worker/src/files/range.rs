use super::relative;
use protocol::attachment::AttachmentBytes;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ByteRange, FileRange};
use std::io;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::Path;

/// Part of the file at `path`, relative to the worktree at `worktree`, as it is now, such as a video's, which a player
/// reads as it needs it. Refused as [`super::read`] is.
pub(crate) fn read_range(worktree: &Path, path: &str, range: ByteRange) -> Result<FileRange, ApiError> {
    relative(path)?;
    let file = crate::worktree_file::find(worktree, path)?;
    let failed = |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't read {path}: {e}"));
    let mut opened = std::fs::File::open(&file).map_err(failed)?;
    let file_bytes = opened.metadata().map_err(failed)?.len();
    let within = range.within(file_bytes);
    opened.seek(SeekFrom::Start(within.start)).map_err(failed)?;
    let mut data = Vec::new();
    opened.take(within.end - within.start).read_to_end(&mut data).map_err(failed)?;
    Ok(FileRange { start: range.start(), data: AttachmentBytes::new(data), file_bytes })
}
