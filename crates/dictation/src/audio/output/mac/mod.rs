//! A Mac's sound output, muted through Core Audio on the device sound plays from as muting starts.

mod device;
mod muting;

pub(crate) use muting::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};
