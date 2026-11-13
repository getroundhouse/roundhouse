//! Telling the system's accessibility which element has keyboard focus, which GPUI's view leaves out on a Mac, as
//! `mac` says.

#[cfg(target_os = "macos")]
mod mac;

mod installing;
#[cfg(target_os = "macos")]
pub(crate) use installing::install;
#[cfg(not(target_os = "macos"))]
pub(crate) use installing::install;
