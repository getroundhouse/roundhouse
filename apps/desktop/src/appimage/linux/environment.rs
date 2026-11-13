use std::ffi::{OsStr, OsString};
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The `AppImage` the app runs from, once `leave_behind` has found it.
pub(in super::super) static RUNNING_FROM: OnceLock<AppImage> = OnceLock::new();

pub(in super::super) struct AppImage {
    pub(in super::super) path: PathBuf,
    /// The descriptors the launcher left the app, which the app keeps out of what it starts.
    pub(in super::super) holding_mount: Vec<RawFd>,
}

pub(in super::super) fn leave_behind() {
    // Only an AppImage's runtime sets it.
    let Some(mount) = std::env::var_os("APPDIR").map(PathBuf::from) else { return };
    let path = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let holding_mount = leave(&inherited(), InChildren::Closed);
    let path_variable = std::env::var_os("PATH").and_then(|path| outside(&path, &mount).ok());
    // SAFETY: nothing else reads or changes the environment as this does, since `main` hasn't started another
    // thread.
    unsafe {
        for name in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            std::env::remove_var(name);
        }
        if let Some(path_variable) = path_variable {
            std::env::set_var("PATH", path_variable);
        }
    }
    if let Some(path) = path {
        // Only the first call finds anything, since it takes `APPDIR` out of the environment.
        let _ = RUNNING_FROM.set(AppImage { path, holding_mount });
    }
}

/// Calls `start`, and leaves `fds` open in what it starts, and closed in what this process starts after.
pub(in super::super) fn sharing<T>(fds: &[RawFd], start: impl FnOnce() -> T) -> T {
    leave(fds, InChildren::Open);
    let started = start();
    // What `start` started has its descriptors already, so this only keeps them out of what comes after.
    leave(fds, InChildren::Closed);
    started
}

/// `path` without its entries inside `mount`.
pub(in super::super) fn outside(path: &OsStr, mount: &Path) -> Result<OsString, std::env::JoinPathsError> {
    std::env::join_paths(std::env::split_paths(path).filter(|entry| !entry.starts_with(mount)))
}

/// The descriptors this process holds besides standard input, output and error, which are the ones it inherited
/// while it has opened none of its own.
pub(in super::super) fn inherited() -> Vec<RawFd> {
    let Ok(entries) = std::fs::read_dir("/proc/self/fd") else { return Vec::new() };
    entries.filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok()).filter(|fd| *fd > 2).collect()
}

/// Whether a descriptor this process holds is open in what it starts.
#[derive(Clone, Copy)]
pub(in super::super) enum InChildren {
    Open,
    Closed,
}

/// Leaves each of `fds` open or closed in whatever this process starts from now on, while it keeps them itself,
/// and says which were open. One that isn't, such as the one that listed them, is left alone.
pub(in super::super) fn leave(fds: &[RawFd], in_children: InChildren) -> Vec<RawFd> {
    fds.iter()
        .copied()
        .filter(|&fd| {
            // SAFETY: fcntl only reads and sets a descriptor's flags, and fails rather than act on one that isn't
            // open.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 {
                    return false;
                }
                let wanted = match in_children {
                    InChildren::Open => flags & !libc::FD_CLOEXEC,
                    InChildren::Closed => flags | libc::FD_CLOEXEC,
                };
                libc::fcntl(fd, libc::F_SETFD, wanted) >= 0
            }
        })
        .collect()
}
