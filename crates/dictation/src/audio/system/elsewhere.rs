//! Windows and Linux, where the microphone isn't heard yet, so [`crate::HEARS`] is false and the app offers no
//! dictation.

use crate::{DictationError, Permission};

/// Never opens: there's no microphone to hear here.
pub(crate) struct Microphone;

impl Microphone {
    pub(crate) fn open(
        _heard: impl FnMut(Vec<f32>) + Send + 'static,
        _failed: impl FnMut(DictationError) + Send + 'static,
    ) -> Result<Self, DictationError> {
        Err(DictationError::NoMicrophone)
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
