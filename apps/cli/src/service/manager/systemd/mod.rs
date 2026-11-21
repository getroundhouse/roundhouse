//! systemd, which runs the hub as a system service, as the hub's user, from boot.

#[cfg(target_os = "linux")]
mod systemctl;
mod unit_file;

#[cfg(target_os = "linux")]
pub(crate) use systemctl::{register, unregister};
