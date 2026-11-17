//! A Mac's sound output, muted through Core Audio on the device sound plays from as muting starts.

mod device;
mod muting;

pub use muting::{MUTES_OUTPUT, output_restored};
pub(crate) use muting::{MutedOutput, mute_output};
