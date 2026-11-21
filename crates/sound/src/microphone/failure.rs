//! Why the microphone couldn't be heard.

/// Why the microphone couldn't be heard, or stopped being heard.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MicrophoneError {
    /// The person hasn't allowed the app to use the microphone, which only the system's settings change.
    #[error("the app isn't allowed to use the microphone")]
    Denied,
    /// The system hasn't asked the person yet, which [`crate::ask_for_microphone`] has it do.
    #[error("the app hasn't asked to use the microphone")]
    NotAsked,
    #[error("there's no microphone")]
    Missing,
    #[error("the microphone stopped working: {0}")]
    Failed(String),
}
