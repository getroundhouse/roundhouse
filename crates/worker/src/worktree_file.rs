//! Finding a file or a folder a client names inside a worktree. A path can lead anywhere through `..` or a link, so it
//! only counts as in the worktree once every link on the way is followed.

use std::io;
use std::path::{Path, PathBuf};

use protocol::client::{ApiError, ErrorCode};

/// The file at `path`, which is absolute or relative to the worktree at `worktree`, in full with every link followed.
/// Refused unless that's a file inside the worktree.
pub(crate) fn find(worktree: &Path, path: &str) -> Result<PathBuf, ApiError> {
    let found = inside(worktree, path)?;
    if !found.is_file() {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a file")));
    }
    Ok(found)
}

/// The folder at `path`, as [`find`] finds a file.
pub(crate) fn find_folder(worktree: &Path, path: &str) -> Result<PathBuf, ApiError> {
    let found = inside(worktree, path)?;
    if !found.is_dir() {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a folder")));
    }
    Ok(found)
}

/// Whatever is at `path`, in full with every link followed, if it's inside the worktree.
fn inside(worktree: &Path, path: &str) -> Result<PathBuf, ApiError> {
    let root = worktree
        .canonicalize()
        .map_err(|e| ApiError::new(ErrorCode::Worker, format!("couldn't find the worktree: {e}")))?;
    let found = match root.join(path).canonicalize() {
        Ok(found) => found,
        // A folder on the way that's a file now, as much as one that's gone, means it isn't there any more.
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {
            return Err(ApiError::new(ErrorCode::NotFound, format!("{path} isn't there")));
        }
        // A name no file can have, such as one with a NUL in it, or one too long to be a name.
        Err(e) if matches!(e.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::InvalidFilename) => {
            return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} can't be found: {e}")));
        }
        Err(e) => return Err(ApiError::new(ErrorCode::Worker, format!("couldn't find {path}: {e}"))),
    };
    if !found.starts_with(&root) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't in the worktree")));
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
