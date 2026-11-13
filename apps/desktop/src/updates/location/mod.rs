//! Where Velopack finds the app and keeps what it downloads, and whether the app is somewhere it can replace itself,
//! which Velopack needs to update it.

#[cfg(test)]
use super::problem::Kind;
#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
mod tests;

mod installation;
pub(super) use installation::cant_install;
#[cfg(target_os = "macos")]
pub(super) use installation::installed;
#[cfg(target_os = "windows")]
pub(super) use installation::installed;
#[cfg(not(target_os = "linux"))]
pub(super) use installation::paths;
#[cfg(target_os = "linux")]
pub(super) use installation::{installed, paths};
