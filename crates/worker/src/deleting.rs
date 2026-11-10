//! Deleting the worktrees archiving moved aside. It goes on after the worker that started it has exited, which the hub
//! has it do as soon as archiving is done, so it's a program of its own: this build's `delete` command, which runs
//! [`delete_trees`].

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tracing::warn;

#[cfg(test)]
mod tests;

/// The program that deletes trees, given their paths after its own arguments.
#[derive(Clone, Debug)]
pub struct Deleter {
    program: PathBuf,
    args: Vec<OsString>,
}

impl Deleter {
    pub fn new(program: impl Into<PathBuf>, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        Self { program: program.into(), args: args.into_iter().map(Into::into).collect() }
    }

    /// Starts deleting `paths`, and returns without waiting. It's in a process group of its own, so nothing that
    /// stops this worker's group stops it. Its errors go to this worker's log, since it shares its stderr.
    pub(crate) fn start(&self, paths: &[PathBuf]) {
        let spawned = Command::new(&self.program)
            .args(&self.args)
            .args(paths)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .process_group(0)
            .spawn();
        match spawned {
            // Waited on to say how it went, while this worker is still here to.
            Ok(mut deleting) => drop(tokio::spawn(async move {
                match deleting.wait().await {
                    Ok(status) if !status.success() => {
                        warn!(%status, "deleting worktrees moved aside failed, and the next archive beside them retries");
                    }
                    Ok(_) => {}
                    Err(e) => warn!("waiting for worktrees moved aside to be deleted: {e}"),
                }
            })),
            Err(e) => warn!(program = %self.program.display(), "couldn't start deleting worktrees moved aside: {e}"),
        }
    }
}

/// Deletes each tree, making its directories writable first if that's what stops it, as with the read-only ones a
/// package cache makes. A tree that's gone already is no failure, since two archives can both be deleting one.
/// Returns what it couldn't delete, and why.
pub fn delete_trees(paths: &[PathBuf]) -> Vec<(PathBuf, io::Error)> {
    paths.iter().filter_map(|path| delete_tree(path).err().map(|e| (path.clone(), e))).collect()
}

fn delete_tree(path: &Path) -> io::Result<()> {
    let refused = match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        done => return done,
    };
    make_writable(path).map_err(|_| refused)?;
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

/// Gives the owner write and search permission on every directory in the tree, without following links out of it.
fn make_writable(dir: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(dir)?;
    if !metadata.is_dir() {
        return Ok(());
    }
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    std::fs::set_permissions(dir, permissions)?;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            make_writable(&entry.path())?;
        }
    }
    Ok(())
}
