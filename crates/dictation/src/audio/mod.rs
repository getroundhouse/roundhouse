//! Where a recording's audio comes from: the microphone, converted to one channel at [`crate::SAMPLE_RATE`] as it's
//! heard, and whether the system lets the app use it.

mod permission;
// Only a Mac's microphone is heard, but the arithmetic is tested everywhere.
#[cfg(any(target_os = "macos", test))]
pub(crate) mod resample;
mod source;
mod system;

pub use permission::Permission;
pub use source::{AudioIn, HEARS};
pub(crate) use system::Microphone;
pub use system::{ask_for_microphone, microphone_permission};
