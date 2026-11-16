//! The model that turns speech into text. It hears a recording twice: as it's said, for live text, and then a sentence
//! at a time, once each has ended, for its final text, heard whole from the pieces the recording was cut into at its
//! pauses.

use crate::{Hints, Samples};

/// A model that turns speech into text. It's loaded, used and dropped on the thread [`crate::Dictation`] keeps for it,
/// so it needn't be `Send`: Core ML's models aren't.
pub trait Engine {
    /// The longest audio [`Engine::transcribe`] takes. A recording is cut into pieces no longer than this, and a sentence
    /// that would be longer is heard whole in parts.
    fn longest_piece(&self) -> Samples;

    /// Starts hearing a recording from its start, for its live text, biased toward `hints`.
    fn listen(&self, hints: &Hints) -> Box<dyn Listening + '_>;

    /// The final text of some of a recording, heard whole, at [`crate::SAMPLE_RATE`], biased toward `hints`: a sentence's
    /// pieces, or as many as fit. It's no longer than [`Engine::longest_piece`]. Empty when nothing was said in it.
    fn transcribe(&self, piece: &[f32], hints: &Hints) -> Result<String, EngineError>;
}

/// One recording, being heard for its live text.
pub trait Listening {
    /// Takes the next of the recording's audio, at [`crate::SAMPLE_RATE`], and gives what it has heard since it was
    /// last asked. It can hold text back until it has heard what follows, so text can come after the audio it's in, but
    /// it gives it in the order it was said, so a mark between two words, such as a sentence's `.`, comes before the word
    /// after it.
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<Heard>, EngineError>;

    /// The recording has ended: what was held back.
    fn finish(&mut self) -> Result<Vec<Heard>, EngineError>;

    /// How far into the recording all of its live text has been given: no text said before this is still to come.
    fn heard_through(&self) -> Samples;
}

/// Some text heard live, and where in the recording it was said, so it can be put in the piece it belongs to however
/// late it comes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    /// As it joins what came before it, such as `" Tokio"` for a new word or `"ing"` for the end of one.
    pub text: String,
    pub at: Samples,
}

/// Why the engine couldn't load or hear, in words for the person.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EngineError(pub String);
