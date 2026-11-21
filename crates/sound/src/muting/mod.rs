//! Muting the system's sound output while something is heard, if the person has chosen it, and putting it back as it
//! was once it stops, each platform's own way.

#[cfg(not(target_os = "macos"))]
mod elsewhere;
#[cfg(target_os = "macos")]
mod mac;

#[cfg(not(target_os = "macos"))]
pub(crate) use elsewhere::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};
#[cfg(target_os = "macos")]
pub(crate) use mac::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};
