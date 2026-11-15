//! Which audio a recording hears.

/// Whether this platform can hear the microphone. A Mac can. Windows and Linux can't yet, so the app offers no
/// dictation there.
pub const HEARS: bool = cfg!(target_os = "macos");

/// Where a recording's audio comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioIn {
    /// The system's default input device, from when the recording starts until it's stopped.
    Microphone,
    /// Only what a test gives the recording with [`crate::Recording::feed`], at [`crate::SAMPLE_RATE`].
    #[cfg(any(test, feature = "testing"))]
    Fed,
}
