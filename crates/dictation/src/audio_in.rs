//! Which audio a recording hears.

/// Where a recording's audio comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioIn {
    /// The system's default input device, from when the recording starts until it's stopped, where
    /// [`sound::HEARS`] says it's heard.
    Microphone,
    /// Only what a test gives the recording with [`crate::Recording::feed`], at [`crate::SAMPLE_RATE`].
    #[cfg(any(test, feature = "testing"))]
    Fed,
}
