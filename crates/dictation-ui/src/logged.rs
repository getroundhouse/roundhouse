//! What the app's log says of a recording, which is never what was said or hinted.

use std::fmt;
use std::time::Instant;

use dictation::{DictationError, Ending};
use uuid::Uuid;

use crate::Place;

/// One recording, as the log says it, with an event as it starts and another as it ends, however it ends, since this
/// goes with the recording.
pub(crate) struct Logged {
    id: RecordingId,
    started: Instant,
    stopped: Option<Instant>,
    how: How,
}

/// Which recording an event is about.
#[derive(Clone, Copy)]
struct RecordingId(Uuid);

impl fmt::Display for RecordingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// How a recording ended.
#[derive(Clone, Copy)]
enum How {
    Heard,
    NothingHeard,
    /// The person cancelled it, and the field was put back as it was.
    Cancelled,
    /// Its field was hidden, or reused for something else, or another field dictated.
    FieldGone,
    Failed,
}

impl How {
    fn as_str(self) -> &'static str {
        match self {
            How::Heard => "heard",
            How::NothingHeard => "nothing_heard",
            How::Cancelled => "cancelled",
            How::FieldGone => "field_gone",
            How::Failed => "failed",
        }
    }
}

impl Logged {
    /// A recording for the field at `place` starts, biased toward `hinted` words.
    pub(crate) fn start(place: Place, hinted: usize) -> Self {
        let id = RecordingId(Uuid::new_v4());
        tracing::info!(
            name: "dictation.started",
            recording_id = %id,
            place = place.as_str(),
            hinted,
            "A recording started"
        );
        // Until something says otherwise, since it's what goes without a word.
        Self { id, started: Instant::now(), stopped: None, how: How::FieldGone }
    }

    /// The last words are being heard, until it ends.
    pub(crate) fn stopped(&mut self) {
        self.stopped = Some(Instant::now());
    }

    pub(crate) fn ended(&mut self, ending: Ending) {
        self.how = match ending {
            Ending::Heard => How::Heard,
            Ending::NothingHeard => How::NothingHeard,
        };
    }

    pub(crate) fn cancelled(&mut self) {
        self.how = How::Cancelled;
    }

    pub(crate) fn failed(&mut self, error: &DictationError) {
        self.how = How::Failed;
        tracing::warn!(name: "dictation.failed", recording_id = %self.id, %error, "A recording failed");
    }
}

impl Drop for Logged {
    fn drop(&mut self) {
        let now = Instant::now();
        let ms = |from: Instant, to: Instant| u64::try_from(to.duration_since(from).as_millis()).unwrap_or(u64::MAX);
        tracing::info!(
            name: "dictation.ended",
            recording_id = %self.id,
            how = self.how.as_str(),
            recorded_ms = ms(self.started, self.stopped.unwrap_or(now)),
            final_text_ms = self.stopped.map(|stopped| ms(stopped, now)),
            "A recording ended"
        );
    }
}

/// The system won't let a recording for the field at `place` hear the microphone, so none started.
pub(crate) fn microphone_denied(place: Place) {
    tracing::warn!(name: "dictation.microphone_denied", place = place.as_str(), "The microphone isn't allowed");
}
