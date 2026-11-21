//! What reading out says as it goes.

/// A text to read out, as its caller names it, such as one block of a reply, or a line said while the agent can't
/// answer. Its places are bytes of the text as it was given, Markdown and all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextId(pub u64);

/// What reading out says, in the order it happened.
#[derive(Debug)]
pub enum Update {
    /// Everything in `text` up to `through` has been heard: a phrase's last sample has gone to the speakers, a moment
    /// before it's heard.
    Heard { text: TextId, through: usize },
    /// All of `text` has been heard, after its last [`Update::Heard`].
    Finished { text: TextId },
    /// Reading stopped, as asked or after a failure: `cut` is the text that was being read and how far it had been
    /// heard, if one had started, and `never_started` the texts after it, none of which was heard, in order. The rest of
    /// those texts is dropped as it comes, but a text first given after the stop is read, even one sent before the
    /// caller heard of it, so a caller that wants nothing more read stops again.
    Stopped { cut: Option<(TextId, usize)>, never_started: Vec<TextId> },
    /// The voice or the speakers failed. A phrase the voice couldn't say is passed over, and what follows it is read,
    /// as it is at the speed it had when the voice couldn't change it. Otherwise what was being read stops, and a
    /// [`Update::Stopped`] follows.
    Failed(SpeakingError),
}

/// Why reading out failed.
#[derive(Debug, thiserror::Error)]
pub enum SpeakingError {
    #[error("the voice couldn't load: {0}")]
    Load(phonon::PhononError),
    #[error("the voice couldn't say a phrase: {0}")]
    Phrase(phonon::PhononError),
    /// The voice couldn't change how fast it speaks, so it speaks as it did.
    #[error("the voice couldn't change its speed: {0}")]
    Speed(phonon::PhononError),
    #[error(transparent)]
    Speakers(#[from] sound::SpeakersError),
}
