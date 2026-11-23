//! Whether the system lets the app do something it guards, such as hear the microphone.

/// What the person has said about the app doing something the system guards, such as using the microphone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Allowed,
    /// Refused, or not theirs to allow, such as on a Mac a profile manages. Only the system's settings change it.
    Denied,
    /// Not asked yet: for the microphone, [`crate::ask_for_microphone`] has the system ask.
    NotAsked,
}
