//! One recording, as whoever started it sees it: a handle that ends it, and what's heard of it.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use parking_lot::Mutex;
use sound::{InputDevice, MicrophoneError, Quieting};

use super::buffer::Buffer;
use crate::{AudioIn, EngineError};

/// One recording, from the moment it's asked for. Dropping it before [`Recording::stop`] cancels it.
pub struct Recording {
    input: Buffer,
    audio: AudioIn,
    stopped: Cell<bool>,
    cancelled: Cancelled,
    /// What [`Recording::quiet`] did, each put back as it was when this lets go of it.
    quieted: RefCell<Vec<Quieted>>,
}

/// What a recording did to quiet what's playing, in one way.
enum Quieted {
    /// Put back as it was once this is dropped.
    Playing(sound::Quieted),
    /// Nothing, since the recording hears only what a test feeds it, which [`Recording::quiets`] tells the test.
    #[cfg(any(test, feature = "testing"))]
    Fed(Quieting),
}

impl Quieted {
    fn how(&self) -> Quieting {
        match self {
            Self::Playing(quieted) => quieted.how(),
            #[cfg(any(test, feature = "testing"))]
            Self::Fed(how) => *how,
        }
    }
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
    pub(super) fn new(input: Buffer, audio: AudioIn, cancelled: Cancelled) -> Self {
        Self { input, audio, stopped: Cell::new(false), cancelled, quieted: RefCell::new(Vec::new()) }
    }

    /// Quiets what's playing `how` says until the recording is stopped or dropped, and then puts it back as it was:
    /// unmutes the sound output, unless it was muted already, or plays what it paused again. Once it's stopped, this
    /// does nothing, and where [`Quieting::is_possible`] is false, a recording from the microphone quiets nothing. A
    /// recording a test feeds quiets nothing either, and [`Recording::quiets`] says whether it was asked to.
    pub fn quiet(&self, how: Quieting) {
        if self.stopped.get() || self.has_quieted(how) {
            return;
        }
        let quieted = match self.audio {
            AudioIn::Microphone => how.quiet().map(Quieted::Playing),
            #[cfg(any(test, feature = "testing"))]
            AudioIn::Fed => Some(Quieted::Fed(how)),
        };
        self.quieted.borrow_mut().extend(quieted);
    }

    /// Whether the recording quiets what's playing `how` says, for a test of a recording it feeds.
    #[cfg(any(test, feature = "testing"))]
    pub fn quiets(&self, how: Quieting) -> bool {
        self.has_quieted(how)
    }

    fn has_quieted(&self, how: Quieting) -> bool {
        self.quieted.borrow().iter().any(|quieted| quieted.how() == how)
    }

    /// A single consumer reads the latest loudness independently of recognition. A slow reader skips old levels.
    /// The UI stops reading when this recording ends; silence sends nothing until its level changes.
    pub fn levels(&self) -> flume::Receiver<crate::Level> {
        self.input.levels()
    }

    /// Ends the recording as letting go of the key does: no more audio is taken, what was said since the last sentence
    /// turned final is heard whole for its final text, and [`Update::Ended`] follows. Dropping this afterwards changes
    /// nothing.
    pub fn stop(&self) {
        self.stopped.set(true);
        self.input.end(Input::Stop);
        // The microphone has closed, so what's playing can be heard again while the last words go in.
        self.quieted.borrow_mut().clear();
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
    /// The microphone is open, hearing this device. It's first, and a recording a test feeds has none.
    Opened(InputDevice),
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
    #[error(transparent)]
    Microphone(#[from] MicrophoneError),
    #[error("speech recognition couldn't keep up with the microphone")]
    AudioBacklog,
    #[error(transparent)]
    Engine(#[from] EngineError),
}
