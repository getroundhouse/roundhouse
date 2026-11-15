//! The microphone and its permission, each platform's own way.

#[cfg(not(target_os = "macos"))]
mod elsewhere;
#[cfg(target_os = "macos")]
mod mac;

#[cfg(not(target_os = "macos"))]
pub(crate) use elsewhere::Microphone;
#[cfg(not(target_os = "macos"))]
pub use elsewhere::{ask_for_microphone, microphone_permission};
#[cfg(target_os = "macos")]
pub(crate) use mac::Microphone;
#[cfg(target_os = "macos")]
pub use mac::{ask_for_microphone, microphone_permission};
