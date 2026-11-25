//! Which audio a recording hears.

/// Where a recording's audio comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioIn {
    /// The system's default input device, from when the recording starts until it's stopped, where
    /// [`sound::HEARS`] says it's heard.
    Microphone,
    /// Only what a test gives the recording with [`crate::Recording::feed`], at [`crate::SAMPLE_RATE`]. It's a case of
    /// its own since a test has no microphone, and no system can be relied on to have one a program can play into. The
    /// product has no other audio to dictate from.
    #[cfg(any(test, feature = "testing"))]
    Fed,
}
