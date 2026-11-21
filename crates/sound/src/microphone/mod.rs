//! The microphone and its permission, each platform's own way.

#[cfg(not(target_os = "macos"))]
mod elsewhere;
mod failure;
mod hears;
#[cfg(target_os = "macos")]
mod mac;

#[cfg(not(target_os = "macos"))]
pub use elsewhere::{Microphone, ask_for_microphone, microphone_permission};
pub use failure::MicrophoneError;
pub use hears::HEARS;
#[cfg(target_os = "macos")]
pub use mac::{Microphone, ask_for_microphone, microphone_permission};
