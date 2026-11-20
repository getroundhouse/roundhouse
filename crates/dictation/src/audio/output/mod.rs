//! The system's sound output, which a recording mutes while it's dictated to, if the person has chosen it, and puts
//! back as it was once it stops, each platform's own way.

#[cfg(not(target_os = "macos"))]
mod elsewhere;
#[cfg(target_os = "macos")]
mod mac;

#[cfg(not(target_os = "macos"))]
pub(crate) use elsewhere::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};
#[cfg(target_os = "macos")]
pub(crate) use mac::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};
