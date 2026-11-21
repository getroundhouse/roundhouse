//! Which platforms hear the microphone.

/// Whether this platform can hear the microphone. A Mac can. Windows and Linux can't yet, so the app offers nothing
/// there that hears it, such as dictation.
pub const HEARS: bool = cfg!(target_os = "macos");
