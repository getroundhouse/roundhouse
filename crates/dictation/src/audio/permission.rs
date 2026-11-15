//! Whether the system lets the app hear the microphone.

/// What the person has said about the app using the microphone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Allowed,
    /// Refused, or not theirs to allow, such as on a Mac a profile manages. Only the system's settings change it.
    Denied,
    /// Not asked yet: [`crate::ask_for_microphone`] has the system ask.
    NotAsked,
}
