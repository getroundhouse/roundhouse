use protocol::attachment::{MediaType, describe_size};
use protocol::client::{ApiError, ErrorCode};
use std::io;
use std::io::Read as _;
use std::path::Path;

/// Far bigger than any screenshot, and small enough that one in a frame is nothing to the hub.
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The image at `path`, which is absolute or relative to the worktree at `worktree`, and what kind it is by its bytes.
pub(crate) fn read(worktree: &Path, path: &str) -> Result<(MediaType, Vec<u8>), ApiError> {
    let refused = |why: &str| ApiError::new(ErrorCode::InvalidRequest, format!("{path} {why}"));
    let file = crate::worktree_file::find(worktree, path)?;
    let failed = |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't read {path}: {e}"));
    let mut bytes = Vec::new();
    std::fs::File::open(&file).and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes)).map_err(failed)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(refused(&format!("is bigger than {}", describe_size(MAX_BYTES))));
    }
    let media_type = MediaType::sniff(&bytes);
    if !media_type.is_image() {
        return Err(refused("isn't an image the transcript draws"));
    }
    Ok((media_type, bytes))
}
