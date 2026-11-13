#[cfg(target_os = "linux")]
use super::linux;
#[cfg(target_os = "linux")]
use std::path::Path;

/// Keeps the launcher's mount, variables and `PATH` entry out of everything the app starts, if it runs from an
/// `AppImage`, but for the mount in what `sharing_mount` starts, and notes where the `AppImage` is, for `path`. It
/// changes the environment, so it comes first in `main`, while the app has one thread.
#[cfg(target_os = "linux")]
pub fn leave_behind() {
    linux::leave_behind();
}

/// Nothing elsewhere, where there's no `AppImage`.
#[cfg(not(target_os = "linux"))]
pub fn leave_behind() {}

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
#[cfg(target_os = "linux")]
pub(crate) fn sharing_mount<T>(start: impl FnOnce() -> T) -> T {
    match linux::RUNNING_FROM.get() {
        Some(appimage) => linux::sharing(&appimage.holding_mount, start),
        None => start(),
    }
}

/// Just calls `start` elsewhere, where there's no `AppImage`.
#[cfg(not(target_os = "linux"))]
pub(crate) fn sharing_mount<T>(start: impl FnOnce() -> T) -> T {
    start()
}
