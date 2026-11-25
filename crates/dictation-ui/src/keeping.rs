//! A recording the person chose to keep, as it goes to what keeps it. Nothing here writes it: the app does.

use std::time::Duration;

use dictation::Language;
use sound::InputDevice;

use crate::{About, Place};

/// What the app takes of a recording to keep, from as it starts to listen. It's let go of unseen if the recording ends
/// before it dictates, as a press of Space too short to dictate does.
pub struct Taken {
    /// All its audio, at [`dictation::SAMPLE_RATE`], until the last of it has come.
    pub audio: flume::Receiver<Vec<f32>>,
    /// The language it's heard in.
    pub language: Language,
}

/// A recording to keep, once it has begun to dictate.
pub struct Keeping {
    /// Its ID, which the log has too.
    pub id: String,
    pub place: Place,
    pub about: About,
    pub taken: Taken,
    /// What's said of it, each with when, from when it began to dictate. [`Said::Ended`] is the last.
    pub said: flume::Receiver<(Duration, Said)>,
}

/// What's said of a recording as it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    Opened(InputDevice),
    Live(String),
    Final(String),
    /// It was stopped, and its last words are still to come.
    Stopped,
    Ended {
        how: How,
        /// Why it failed, if it did.
        error: Option<String>,
    },
}

/// How a recording ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Heard,
    NothingHeard,
    /// The person cancelled it, and the field was put back as it was.
    Cancelled,
    /// Its field was hidden, or reused for something else, or another field dictated.
    FieldGone,
    Failed,
}

impl How {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            How::Heard => "heard",
            How::NothingHeard => "nothing_heard",
            How::Cancelled => "cancelled",
            How::FieldGone => "field_gone",
            How::Failed => "failed",
        }
    }
}
