//! What an encoder made out of one window of audio, laid out a row per feature across the window's steps.

use super::super::parts::Tensor;
use super::super::transducer::ENCODED;
use super::super::windows::Steps;
use crate::EngineError;

pub(in super::super) struct Encoded {
    /// `ENCODED` rows of `steps` values.
    values: Vec<f32>,
    steps: usize,
    /// Where the window starts in the recording.
    first: Steps,
    /// How many of its steps are audio, from the first.
    audio: Steps,
}

impl Encoded {
    /// The encoder's `encoder` and `encoder_length` outputs, for a window that starts at `first` in the recording.
    pub(in super::super) fn new(encoder: Tensor, length: &Tensor, first: Steps) -> Result<Self, EngineError> {
        let steps = match encoder.shape.as_slice() {
            [1, values, steps] if *values == ENCODED && encoder.values.len() == ENCODED * steps => *steps,
            shape => return Err(EngineError(format!("an encoder gave a shape of {shape:?}"))),
        };
        Ok(Self { values: encoder.values, steps, first, audio: Steps(length.whole()?.min(steps)) })
    }

    /// Where what it made out of the window's audio ends in the recording.
    pub(in super::super) fn made_out(&self) -> Steps {
        Steps(self.first.0 + self.audio.0)
    }

    /// What it made out of the step `at`, a place in the recording within the window's audio.
    pub(super) fn step(&self, at: Steps) -> Vec<f32> {
        let step = at.0 - self.first.0;
        (0..ENCODED).map(|value| self.values[value * self.steps + step]).collect()
    }
}
