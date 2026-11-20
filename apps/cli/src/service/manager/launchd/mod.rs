//! launchd, which runs the hub as a launch daemon: in the system's domain, as the hub's user, from boot.

#[cfg(target_os = "macos")]
mod launchctl;
mod property_list;

#[cfg(target_os = "macos")]
pub(crate) use launchctl::{register, unregister};
