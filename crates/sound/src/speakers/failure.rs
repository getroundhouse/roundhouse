//! Why the speakers couldn't play.

/// Why the speakers couldn't play, or stopped playing.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpeakersError {
    #[error("there's no sound output")]
    Missing,
    #[error("the sound output stopped working: {0}")]
    Failed(String),
}
