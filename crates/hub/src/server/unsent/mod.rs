//! How much of what's written to a device's connection the kernel holds unsent, as `network` sets it, each platform
//! its own way.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(all(target_os = "linux", test))]
pub(super) use linux::most_unsent;
#[cfg(target_os = "linux")]
pub(super) use linux::set_most_unsent;
#[cfg(all(target_os = "macos", test))]
pub(super) use macos::most_unsent;
#[cfg(target_os = "macos")]
pub(super) use macos::set_most_unsent;
#[cfg(windows)]
pub(super) use windows::set_most_unsent;
