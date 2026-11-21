//! The engine, loaded, and the speech it makes of a text.

use std::path::Path;

use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions, SpeechStream, Synth};

/// The voice it speaks in: the model's own, from its files.
const VOICE: &str = "default";

/// Phonon 31m, on the CPU, in its own voice. It can be used from any thread, and speaks one text at a time.
pub struct Phonon {
    synth: Synth,
}

/// Why the engine couldn't load or speak.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct PhononError(#[from] ptts::Error);

impl Phonon {
    /// The engine, from its files in `dir`, as [`crate::MODEL`] lists them. It speaks a word before it returns, since
    /// its voice is made ready on first use, which would otherwise come out of the first phrase's time.
    pub fn load(dir: &Path) -> Result<Self, PhononError> {
        let checkpoint = Checkpoint::resolve(dir, ResolveOptions { quant: Quant::Q80, weights: None })?;
        let mut synth = checkpoint.builder(Lang::En).device(DeviceKind::Cpu).build()?;
        checkpoint.register_voices(&mut synth);
        let phonon = Self { synth };
        for audio in phonon.speak("Hi.")? {
            audio?;
        }
        Ok(phonon)
    }

    /// The rate of the audio it makes, in samples a second.
    pub fn sample_rate(&self) -> u32 {
        self.synth.sample_rate()
    }

    /// Starts speaking `text`, which comes as it's made. Dropping it stops the rest being made.
    pub fn speak(&self, text: &str) -> Result<Speech, PhononError> {
        Ok(Speech(self.synth.stream_with(text, &SpeechOptions::default().voice(VOICE))?))
    }
}

/// A text being spoken: its audio, in one channel at [`Phonon::sample_rate`], a piece at a time as it's made.
pub struct Speech(SpeechStream);

impl Iterator for Speech {
    type Item = Result<Vec<f32>, PhononError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|audio| audio.map_err(PhononError::from))
    }
}
