//! One recording, as whoever started it sees it: a handle that ends it, and what's heard of it.

use std::cell::Cell;
use std::sync::Arc;

use parking_lot::Mutex;

use super::buffer::Buffer;
use crate::EngineError;

/// One recording, from the moment it's asked for. Dropping it before [`Recording::stop`] cancels it.
pub struct Recording {
    input: Buffer,
    stopped: Cell<bool>,
    cancelled: Cancelled,
}

/// Whether a recording has been cancelled, which its loop reads before each thing it does, since what's already
/// queued for it comes before the cancel does.
#[derive(Clone, Debug, Default)]
pub(super) struct Cancelled(Arc<Mutex<bool>>);

impl Cancelled {
    pub(super) fn is_set(&self) -> bool {
        *self.0.lock()
    }

    /// Sends `update` unless the recording has been cancelled, holding the flag while it does, so nothing is sent once
    /// [`Recording::cancel`] has returned.
    pub(super) fn send_unless_set(&self, updates: &flume::Sender<Update>, update: Update) {
        let cancelled = self.0.lock();
        if !*cancelled {
            let _ = updates.send(update);
        }
    }

    fn set(&self) {
        *self.0.lock() = true;
    }
}

/// What reaches a recording's loop, in the order it happened.
pub(super) enum Input {
    /// More of its audio, at [`crate::SAMPLE_RATE`].
    Audio(Vec<f32>),
    /// The microphone stopped working.
    Failed(DictationError),
    Stop,
    Cancel,
}

impl Recording {
    pub(super) fn new(input: Buffer, cancelled: Cancelled) -> Self {
        Self { input, stopped: Cell::new(false), cancelled }
    }

    /// Ends the recording as letting go of the key does: no more audio is taken, what was said since the last sentence
    /// turned final is heard whole for its final text, and [`Update::Ended`] follows. Dropping this afterwards changes
    /// nothing.
    pub fn stop(&self) {
        self.stopped.set(true);
        self.input.end(Input::Stop);
    }

    /// Ends the recording at once, and nothing more is heard of it, not even of what was already said: no more
    /// updates, and no [`Update::Ended`]. After [`Recording::stop`], it changes nothing.
    pub fn cancel(self) {
        drop(self);
    }

    /// Gives the recording audio, at [`crate::SAMPLE_RATE`], as a microphone would.
    #[cfg(any(test, feature = "testing"))]
    pub fn feed(&self, audio: Vec<f32>) {
        self.input.audio(audio);
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if !self.stopped.get() {
            self.cancelled.set();
            // Wakes the loop, if it's waiting for audio.
            self.input.end(Input::Cancel);
        }
    }
}

/// What's heard of a recording, in order. Nothing follows [`Update::Ended`] or [`Update::Failed`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// The live text of what's been said since the last sentence turned final: all of it each time, so one missed loses
    /// nothing, and empty once there's none.
    Hearing(String),
    /// One sentence's final text, or part of one too long to hear whole, which comes in place of the live text it had,
    /// to go where it was being said.
    Final(String),
    /// The recording was stopped, and everything said in it is final.
    Ended(Ending),
    /// The recording ended because something failed. What was final before it stays so.
    Failed(DictationError),
}

/// How a recording that was stopped went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// Some final text came of it.
    Heard,
    /// Nothing was said in it, or nothing that could be made out.
    NothingHeard,
}

/// Why a recording couldn't be heard.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DictationError {
    /// The person hasn't allowed the app to use the microphone, which only the system's settings change.
    #[error("the app isn't allowed to use the microphone")]
    MicrophoneDenied,
    /// The system hasn't asked the person yet, which [`crate::ask_for_microphone`] has it do.
    #[error("the app hasn't asked to use the microphone")]
    MicrophoneNotAsked,
    #[error("there's no microphone")]
    NoMicrophone,
    #[error("the microphone stopped working: {0}")]
    Microphone(String),
    #[error("speech recognition couldn't keep up with the microphone")]
    AudioBacklog,
    #[error(transparent)]
    Engine(#[from] EngineError),
}
