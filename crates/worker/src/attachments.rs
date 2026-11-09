//! Where a prompt's files go in its worktree, for the agent to open: `.context/attachments/`. The folder holds a
//! `.gitignore` that ignores everything in it, itself included, so the files never show as changes, whatever the
//! repository ignores, and nothing about the repository has to change. They go with the worktree when it's archived.

use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::client::{ApiError, ErrorCode};

/// Where in a worktree its prompts' files go.
pub const DIR: &str = ".context/attachments";

/// How many different files with one name a worktree keeps before refusing another.
const MAX_SAME_NAME: u32 = 1000;

/// Makes each write's partial file its own.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// Writes `data` as `name` in the worktree at `worktree`, and returns where it went, in full. A file there with that
/// name and these bytes is this file, so writing it again finds it. One with other bytes keeps its name, and this one
/// is numbered, as `build (2).log`.
pub fn write(worktree: &Path, name: &str, data: &[u8]) -> Result<PathBuf, ApiError> {
    let name = protocol::attachment::file_name(name);
    let failed = |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't put {name} in the worktree: {e}"));
    let dir = folder(worktree).map_err(failed)?;
    // Written whole under a name of its own, then linked into place, so the file under its own name is never half
    // written, even if the worker stops part way.
    let partial = dir.join(format!(".partial-{}-{}", std::process::id(), WRITES.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&partial, data).map_err(failed)?;
    let placed = place(&dir, &name, &partial, data);
    let _ = std::fs::remove_file(&partial);
    placed.map_err(failed)
}

/// The worktree's attachments folder, made if it isn't there yet, with a `.gitignore` of its own. Either folder being
/// a link is refused, or a repository could point it anywhere and have files written there.
fn folder(worktree: &Path) -> io::Result<PathBuf> {
    let mut dir = worktree.to_path_buf();
    for part in DIR.split('/') {
        dir.push(part);
        match std::fs::symlink_metadata(&dir) {
            Ok(found) if found.is_dir() => {}
            Ok(_) => return Err(io::Error::other(format!("{} isn't a folder", dir.display()))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => match std::fs::create_dir(&dir) {
                Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
                _ => {}
            },
            Err(e) => return Err(e),
        }
    }
    // Never written through anything already there, a link included.
    match std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(".gitignore")) {
        Ok(mut ignore) => ignore.write_all(b"*\n")?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    Ok(dir)
}

/// Only ever adds a name, so it can't replace a file another prompt put there meanwhile.
fn place(dir: &Path, name: &str, partial: &Path, data: &[u8]) -> io::Result<PathBuf> {
    for n in 1..=MAX_SAME_NAME {
        let target = dir.join(numbered(name, n));
        match std::fs::hard_link(partial, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if holds(&target, data)? {
                    return Ok(target);
                }
            }
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!("there are already {MAX_SAME_NAME} different files called {name}")))
}

/// `build.log`, then `build (2).log`, `build (3).log` and so on.
fn numbered(name: &str, n: u32) -> String {
    if n == 1 {
        return name.to_string();
    }
    let path = Path::new(name);
    match (path.file_stem().and_then(|s| s.to_str()), path.extension().and_then(|s| s.to_str())) {
        (Some(stem), Some(extension)) => format!("{stem} ({n}).{extension}"),
        _ => format!("{name} ({n})"),
    }
}

/// Whether `path` is a file holding exactly `data`. Anything else there, such as a folder, isn't.
fn holds(path: &Path, data: &[u8]) -> io::Result<bool> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() != u64::try_from(data.len()).unwrap_or(u64::MAX) {
        return Ok(false);
    }
    let mut there = Vec::with_capacity(data.len());
    std::fs::File::open(path)?.read_to_end(&mut there)?;
    Ok(there == data)
}

#[cfg(test)]
mod tests;
