//! Reading an image in a worktree, so a client can draw one that the agent's text points to. Text an agent writes is
//! untrusted, so only a file inside the worktree can be read, once every link on the way is followed, and only an
//! image the transcript draws, so nothing else in the worktree or on the machine can be read this way.

use std::io::{self, Read as _};
use std::path::Path;

use protocol::attachment::{MediaType, describe_size};
use protocol::client::{ApiError, ErrorCode};

/// Far bigger than any screenshot, and small enough that one in a frame is nothing to the hub.
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The image at `path`, which is absolute or relative to the worktree at `worktree`, and what kind it is by its bytes.
pub(crate) fn read(worktree: &Path, path: &str) -> Result<(MediaType, Vec<u8>), ApiError> {
    let refused = |why: &str| ApiError::new(ErrorCode::InvalidRequest, format!("{path} {why}"));
    let root = worktree
        .canonicalize()
        .map_err(|e| ApiError::new(ErrorCode::Worker, format!("couldn't find the worktree: {e}")))?;
    let file = match root.join(path).canonicalize() {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(ApiError::new(ErrorCode::NotFound, format!("{path} isn't there")));
        }
        Err(e) => return Err(ApiError::new(ErrorCode::Worker, format!("couldn't find {path}: {e}"))),
    };
    if !file.starts_with(&root) {
        return Err(refused("isn't in the worktree"));
    }
    if !file.is_file() {
        return Err(refused("isn't a file"));
    }
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

#[cfg(test)]
mod tests;
