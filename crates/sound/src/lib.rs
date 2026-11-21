//! The device's sound: the microphone, converted to one channel at the rate its caller asks for as it's heard, whether
//! the system lets the app use it, and quieting what's playing, by muting the sound output or pausing what the media
//! keys control. It knows nothing of speech, so dictation and anything else that hears or plays can share it. No UI and
//! no async runtime, as `docs/principles/layers.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod changes;
mod media;
mod microphone;
mod muting;
mod permission;
mod quieting;
// Only a Mac's microphone is heard, but the arithmetic is tested everywhere.
#[cfg(any(target_os = "macos", test))]
mod resample;

pub use microphone::{HEARS, Microphone, MicrophoneError, ask_for_microphone, microphone_permission};
pub use permission::Permission;
pub use quieting::{Quieted, Quieting, restored};
