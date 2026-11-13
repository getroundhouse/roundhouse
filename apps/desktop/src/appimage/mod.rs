//! What an `AppImage`'s launcher leaves the app, kept out of what the app starts. Its runtime mounts the app's
//! files, and holds the mount up through descriptors it leaves open across `exec`. It sets `APPDIR`, `APPIMAGE`,
//! `ARGV0` and `OWD`, and Velopack's `AppRun` puts the mount's `usr/bin` first on `PATH`. The app starts a hub that
//! outlives it, which would otherwise hold the mount up for as long as it runs, and pass the rest on to every worker
//! and agent, whose `roundhouse` would be the one inside whichever version started the hub. Velopack's updater is the
//! one thing the app starts that needs the mount, which `sharing_mount` gives it. Which descriptors a process passes
//! on is the process's own, so what this module finds is kept for the process, not handed around.

mod mount;
#[cfg(target_os = "linux")]
pub use mount::leave_behind;
#[cfg(not(target_os = "linux"))]
pub use mount::leave_behind;
#[cfg(not(target_os = "linux"))]
pub(crate) use mount::sharing_mount;
#[cfg(target_os = "linux")]
pub(crate) use mount::{path, sharing_mount};
#[cfg(target_os = "linux")]
mod linux;
