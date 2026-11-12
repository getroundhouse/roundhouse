//! What an `AppImage`'s launcher leaves the app, kept out of what the app starts. Its runtime mounts the app's
//! files, and holds the mount up through descriptors it leaves open across `exec`. It sets `APPDIR`, `APPIMAGE`,
//! `ARGV0` and `OWD`, and Velopack's `AppRun` puts the mount's `usr/bin` first on `PATH`. The app starts a hub that
//! outlives it, which would otherwise hold the mount up for as long as it runs, and pass the rest on to every worker
//! and agent, whose `roundhouse` would be the one inside whichever version started the hub. Velopack's updater is the
//! one thing the app starts that needs the mount, which `sharing_mount` gives it. Which descriptors a process passes
//! on is the process's own, so what this module finds is kept for the process, not handed around.

#[cfg(target_os = "linux")]
use std::path::Path;

/// Keeps the launcher's mount, variables and `PATH` entry out of everything the app starts, if it runs from an
/// `AppImage`, but for the mount in what `sharing_mount` starts, and notes where the `AppImage` is, for `path`. It
/// changes the environment, so it comes first in `main`, while the app has one thread.
pub fn leave_behind() {
    #[cfg(target_os = "linux")]
    linux::leave_behind();
}

/// The `AppImage` the app runs from, if it runs from one, which Velopack replaces when it updates the app. Velopack
/// would read it from `APPIMAGE`, which `leave_behind` takes out of the environment, so it's known once that has run.
#[cfg(target_os = "linux")]
pub(crate) fn path() -> Option<&'static Path> {
    linux::RUNNING_FROM.get().map(|appimage| appimage.path.as_path())
}

/// Calls `start`, and has what it starts hold up the mount, if the app runs from an `AppImage`, as Velopack's updater
/// needs. The updater runs from inside the mount and waits for the app to quit, so the mount would go with the app,
/// and take the updater with it before it had replaced anything. Whatever another thread starts meanwhile holds the
/// mount up too.
pub(crate) fn sharing_mount<T>(start: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = linux::RUNNING_FROM.get() {
        return linux::sharing(&appimage.holding_mount, start);
    }
    start()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{OsStr, OsString};
    use std::os::fd::RawFd;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    /// The `AppImage` the app runs from, once `leave_behind` has found it.
    pub(super) static RUNNING_FROM: OnceLock<AppImage> = OnceLock::new();

    pub(super) struct AppImage {
        pub(super) path: PathBuf,
        /// The descriptors the launcher left the app, which the app keeps out of what it starts.
        pub(super) holding_mount: Vec<RawFd>,
    }

    pub(super) fn leave_behind() {
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
    pub(super) fn sharing<T>(fds: &[RawFd], start: impl FnOnce() -> T) -> T {
        leave(fds, InChildren::Open);
        let started = start();
        // What `start` started has its descriptors already, so this only keeps them out of what comes after.
        leave(fds, InChildren::Closed);
        started
    }

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

    /// Whether a descriptor this process holds is open in what it starts.
    #[derive(Clone, Copy)]
    pub(super) enum InChildren {
        Open,
        Closed,
    }

    /// Leaves each of `fds` open or closed in whatever this process starts from now on, while it keeps them itself,
    /// and says which were open. One that isn't, such as the one that listed them, is left alone.
    pub(super) fn leave(fds: &[RawFd], in_children: InChildren) -> Vec<RawFd> {
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

    #[cfg(test)]
    mod tests;
}
