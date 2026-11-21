//! What the thread speaks with and plays through, as traits, so its tests can stand in for the model and the device.

use phonon::{Phonon, PhononError, Speed};
use sound::{Played, Speakers};

/// A phrase's audio, a piece at a time as it's made, in one channel at its voice's rate.
pub(crate) type Speech = Box<dyn Iterator<Item = Result<Vec<f32>, PhononError>>>;

/// A voice, loaded: Phonon, or a test's.
pub(crate) trait Voice {
    /// The rate of the audio it makes, in samples a second.
    fn sample_rate(&self) -> u32;

    /// Starts saying `text`. Dropping what it gives stops the rest being made.
    fn speak(&self, text: &str) -> Result<Speech, PhononError>;

    /// Speaks at `speed` from the next text on.
    fn set_speed(&mut self, speed: Speed) -> Result<(), PhononError>;
}

impl Voice for Phonon {
    fn sample_rate(&self) -> u32 {
        Self::sample_rate(self)
    }

    fn speak(&self, text: &str) -> Result<Speech, PhononError> {
        Ok(Box::new(Self::speak(self, text)?))
    }

    fn set_speed(&mut self, speed: Speed) -> Result<(), PhononError> {
        Self::set_speed(self, speed)
    }
}

/// Where the audio plays, once opened: the speakers, or a test's. What it reached before it's closed goes with it.
pub(crate) trait Output {
    fn play(&mut self, audio: &[f32]);
    fn mark(&mut self, mark: u64);
    fn stop(&mut self);
    fn played(&self) -> &flume::Receiver<Played>;
}

impl Output for Speakers {
    fn play(&mut self, audio: &[f32]) {
        Self::play(self, audio);
    }

    fn mark(&mut self, mark: u64) {
        Self::mark(self, mark);
    }

    fn stop(&mut self) {
        Self::stop(self);
    }

    fn played(&self) -> &flume::Receiver<Played> {
        Self::played(self)
    }
}
