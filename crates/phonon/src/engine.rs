//! The engine, loaded, and the speech it makes of a text.

use std::collections::HashMap;
use std::path::Path;

use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions, SpeechStream, Synth};

use crate::Speed;

/// The voice it speaks in: the model's own, from its files.
const VOICE: &str = "default";

/// The model's condition that says how much time it gives each sound, which [`Speed`] sets.
const DURATION: &str = "duration_delta";

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
        phonon.make_ready()?;
        Ok(phonon)
    }

    /// Speaks at `speed` from the next text on. The voice is made ready again, as loading does, since changing its
    /// pace undoes that, so the next phrase is as quick to start as any.
    pub fn set_speed(&mut self, speed: Speed) -> Result<(), PhononError> {
        self.synth.set_conditions(HashMap::from([(DURATION.to_string(), speed.duration_delta().to_string())]))?;
        self.make_ready()
    }

    /// Speaks a word, and drops it, which readies the voice for what's said next.
    fn make_ready(&self) -> Result<(), PhononError> {
        for audio in self.speak("Hi.")? {
            audio?;
        }
        Ok(())
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
