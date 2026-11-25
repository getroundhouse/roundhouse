//! Windows and Linux, where the microphone isn't heard yet, so [`crate::HEARS`] is false and the app offers nothing
//! that hears it.

use super::{InputDevice, MicrophoneError};
use crate::Permission;

/// Never opens: there's no microphone to hear here.
pub enum Microphone {}

impl Microphone {
    pub fn open(
        _rate: u32,
        _heard: impl FnMut(Vec<f32>) + Send + 'static,
        _failed: impl FnMut(MicrophoneError) + Send + 'static,
    ) -> Result<Self, MicrophoneError> {
        Err(MicrophoneError::Missing)
    }

    /// The device it hears.
    pub fn device(&self) -> &InputDevice {
        match *self {}
    }
}

/// Allowed: neither system asks a person before an app records, as a Mac does.
pub fn microphone_permission() -> Permission {
    Permission::Allowed
}

/// Answers at once that it's allowed, as [`microphone_permission`] says.
pub fn ask_for_microphone(answered: impl FnOnce(bool) + Send + 'static) {
    answered(true);
}
