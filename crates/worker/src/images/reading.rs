use protocol::attachment::{MAX_FILE_BYTES, MediaType, describe_size};
use protocol::client::{ApiError, ErrorCode};
use std::io;
use std::io::Read as _;
use std::path::Path;

/// Far bigger than any screenshot, and small enough that one in a frame is nothing to the hub.
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// The image or video at `path`, which is absolute or relative to the worktree at `worktree`, and what kind it is by
/// its bytes, if it's no bigger than `room`. A video can be as big as any file a session keeps, since a recording of a
/// few minutes can be tens of megabytes, where an image is at most a third of that. No more of it is read than it can
/// be.
pub(crate) fn read(worktree: &Path, path: &str, room: u64) -> Result<(MediaType, Vec<u8>), ApiError> {
    let refused = |why: &str| ApiError::new(ErrorCode::InvalidRequest, format!("{path} {why}"));
    let file = crate::worktree_file::find(worktree, path)?;
    let failed = |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't read {path}: {e}"));
    let opened = std::fs::File::open(&file).map_err(failed)?;
    let mut bytes = Vec::new();
    let first = MAX_IMAGE_BYTES.min(room) + 1;
    (&opened).take(first).read_to_end(&mut bytes).map_err(failed)?;
    let media_type = MediaType::sniff(&bytes);
    if !media_type.is_shown_in_transcript() {
        return Err(refused("isn't an image or a video the transcript shows"));
    }
    let most = if media_type.is_video() { MAX_FILE_BYTES } else { MAX_IMAGE_BYTES }.min(room);
    (&opened).take((most + 1).saturating_sub(first)).read_to_end(&mut bytes).map_err(failed)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > most {
        return Err(refused(&format!("is bigger than {}", describe_size(most))));
    }
    Ok((media_type, bytes))
}
