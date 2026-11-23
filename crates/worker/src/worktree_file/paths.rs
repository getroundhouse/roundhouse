use protocol::client::{ApiError, ErrorCode};
use std::io;
use std::path::{Component, Path, PathBuf};

/// Whether the last name of a path is followed where it's a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Last {
    /// To what it leads to, as opening a file does.
    Followed,
    /// Taken as itself, as git takes a link it tracks, so it's inside the worktree once the folders on the way are,
    /// and needn't be there.
    AsItself,
}

/// `path`, written as a list of the worktree's files gives it: names between single slashes, from the top. Refused if
/// it's absolute, or has `.` or `..` in it, which would be read from somewhere else, or an empty name, which would
/// name it differently from how it's listed, or is empty itself.
pub(crate) fn relative(path: &str) -> Result<(), ApiError> {
    let names = path.split('/').all(|name| !matches!(name, "" | "." | ".."));
    let parts = Path::new(path).components();
    if !names || !parts.into_iter().all(|part| matches!(part, Component::Normal(_))) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a path in the worktree")));
    }
    Ok(())
}

/// The file at `path`, which is absolute or relative to the worktree at `worktree`, in full with every link followed.
/// Refused unless that's a file inside the worktree.
pub(crate) fn find(worktree: &Path, path: &str) -> Result<PathBuf, ApiError> {
    let found = inside(worktree, path, Last::Followed)?;
    if !found.is_file() {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a file")));
    }
    Ok(found)
}

/// The folder at `path`, as [`find`] finds a file.
pub(crate) fn find_folder(worktree: &Path, path: &str) -> Result<PathBuf, ApiError> {
    let found = inside(worktree, path, Last::Followed)?;
    if !found.is_dir() {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't a folder")));
    }
    Ok(found)
}

/// Where `path`, which is absolute or relative to the worktree at `worktree`, leads, in full with every link on the
/// way followed, and the last as `last` says. Refused unless that's inside the worktree, and `NotFound` where a folder
/// on the way isn't there, or with the last followed, the last isn't.
pub(crate) fn inside(worktree: &Path, path: &str, last: Last) -> Result<PathBuf, ApiError> {
    let root = worktree
        .canonicalize()
        .map_err(|e| ApiError::new(ErrorCode::Worker, format!("couldn't find the worktree: {e}")))?;
    let named = root.join(path);
    let found = match last {
        Last::Followed => canonical(&named, path)?,
        Last::AsItself => {
            let (Some(folder), Some(name)) = (named.parent(), named.file_name()) else {
                return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} names no file")));
            };
            canonical(folder, path)?.join(name)
        }
    };
    if !found.starts_with(&root) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} isn't in the worktree")));
    }
    Ok(found)
}

/// `named`, which the request named as `path`, in full with every link followed.
fn canonical(named: &Path, path: &str) -> Result<PathBuf, ApiError> {
    match named.canonicalize() {
        Ok(found) => Ok(found),
        // A folder on the way that's a file now, as much as one that's gone, means it isn't there any more.
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {
            Err(ApiError::new(ErrorCode::NotFound, format!("{path} isn't there")))
        }
        // A name no file can have, such as one with a NUL in it, or one too long to be a name.
        Err(e) if matches!(e.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::InvalidFilename) => {
            Err(ApiError::new(ErrorCode::InvalidRequest, format!("{path} can't be found: {e}")))
        }
        Err(e) => Err(ApiError::new(ErrorCode::Worker, format!("couldn't find {path}: {e}"))),
    }
}
