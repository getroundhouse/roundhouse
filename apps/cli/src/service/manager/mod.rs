//! Registering a hub installed as a service with this machine's service manager, and removing it: launchd on a Mac and
//! systemd on Linux, each in a module of its own that alone knows its files and commands. Windows has none yet. What
//! each writes is built for tests everywhere, as `docs/standards/platforms.md` asks, since it needs no platform.

#[cfg(unix)]
mod command;
#[cfg(any(target_os = "macos", test))]
mod launchd;
#[cfg(unix)]
mod system_file;
#[cfg(any(target_os = "linux", test))]
mod systemd;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub(crate) use launchd::{register, unregister};
#[cfg(target_os = "linux")]
pub(crate) use systemd::{register, unregister};
#[cfg(windows)]
pub(crate) use windows::{register, unregister};
