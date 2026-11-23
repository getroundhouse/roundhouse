//! Parts of a world's files, read as the hub and the worker read them, for a fake hub to answer with.

use protocol::attachment::AttachmentBytes;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ByteRange, FileRange};

use super::world::World;

/// Part of the attachment `data`, or of a video in the worktree, `length` bytes from `start`, refused as the hub and
/// the worker refuse it.
pub(super) fn part(data: Option<&Vec<u8>>, start: u64, length: u32) -> Result<FileRange, ApiError> {
    let range = ByteRange::new(start, length)?;
    let data = data.ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
    let file_bytes = u64::try_from(data.len()).unwrap();
    let within = range.within(file_bytes);
    let taken = data[usize::try_from(within.start).unwrap()..usize::try_from(within.end).unwrap()].to_vec();
    Ok(FileRange { start, data: AttachmentBytes::new(taken), file_bytes })
}

/// The bytes of the video at `path` in the world's worktree.
pub(super) fn worktree_video<'a>(world: &'a World, path: &str) -> Option<&'a Vec<u8>> {
    world.worktree_videos.get(path)
}
