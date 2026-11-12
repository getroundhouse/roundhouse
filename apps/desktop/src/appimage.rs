//! What an `AppImage`'s launcher leaves the app, kept out of what the app starts. Its runtime mounts the app's
//! files, and holds the mount up through a descriptor it leaves open across `exec`. It sets `APPDIR`, `APPIMAGE`,
//! `ARGV0` and `OWD`, and Velopack's `AppRun` puts the mount's `usr/bin` first on `PATH`. The app starts a hub that
//! outlives it, which would otherwise hold the mount up for as long as it runs, and pass the rest on to every worker
//! and agent, whose `roundhouse` would be the one inside whichever version started the hub.

use std::path::{Path, PathBuf};

/// The `AppImage` file the app runs from, which Velopack replaces when it updates the app.
#[derive(Debug)]
pub struct AppImage(PathBuf);

impl AppImage {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Keeps the launcher's mount, variables and `PATH` entry out of everything the app starts, if it runs from an
/// `AppImage`, and says where the `AppImage` is, since Velopack would otherwise read that from `APPIMAGE`. It changes
/// the environment, so it comes first in `main`, while the app has one thread.
#[cfg(target_os = "linux")]
pub fn leave_behind() -> Option<AppImage> {
    // Only an AppImage's runtime sets it.
    let mount = PathBuf::from(std::env::var_os("APPDIR")?);
    let appimage = std::env::var_os("APPIMAGE").map(|path| AppImage(PathBuf::from(path)));
    linux::keep_from_children(linux::inherited());
    let path = std::env::var_os("PATH").and_then(|path| linux::outside(&path, &mount).ok());
    // SAFETY: nothing else reads or changes the environment as this does, since `main` hasn't started another thread.
    unsafe {
        for name in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            std::env::remove_var(name);
        }
        if let Some(path) = path {
            std::env::set_var("PATH", path);
        }
    }
    appimage
}

/// Only Linux has `AppImage`s.
#[cfg(not(target_os = "linux"))]
pub fn leave_behind() -> Option<AppImage> {
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{OsStr, OsString};
    use std::os::fd::RawFd;
    use std::path::Path;

    /// `path` without its entries inside `mount`.
    pub(super) fn outside(path: &OsStr, mount: &Path) -> Result<OsString, std::env::JoinPathsError> {
        std::env::join_paths(std::env::split_paths(path).filter(|entry| !entry.starts_with(mount)))
    }

    /// The descriptors this process holds besides standard input, output and error, which are the ones it inherited
    /// while it has opened none of its own.
    pub(super) fn inherited() -> Vec<RawFd> {
        let Ok(entries) = std::fs::read_dir("/proc/self/fd") else { return Vec::new() };
        entries.filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok()).filter(|fd| *fd > 2).collect()
    }

    /// Has each of `fds` closed in whatever this process starts, while it keeps them itself. A descriptor that's no
    /// longer open, such as the one that listed them, is left alone.
    pub(super) fn keep_from_children(fds: impl IntoIterator<Item = RawFd>) {
        for fd in fds {
            // SAFETY: fcntl only reads and sets a descriptor's flags, and fails rather than act on one that isn't open.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags >= 0 {
                    libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                }
            }
        }
    }

    #[cfg(test)]
    mod tests;
}
