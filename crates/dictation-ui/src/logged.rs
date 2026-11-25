//! What the app's log says of a recording, which is never what was said or hinted.

use std::fmt;
use std::time::Instant;

use dictation::{DictationError, Ending};
use sound::InputDevice;
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

    /// The microphone is open, hearing `device`.
    pub(crate) fn opened(&self, device: &InputDevice) {
        let InputDevice { name, sample_rate, channels } = device;
        tracing::info!(
            name: "dictation.microphone_opened",
            recording_id = %self.id,
            device = name,
            sample_rate,
            channels,
            "The microphone opened"
        );
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

#[cfg(test)]
mod tests {
    use logging::testing::Logs;
    use sound::InputDevice;

    use super::Logged;
    use crate::Place;

    #[test]
    fn the_device_a_recording_hears_is_logged_with_the_recordings_id() {
        let logs = Logs::capture();

        let logged = Logged::start(Place::Composer, 0);
        logged.opened(&InputDevice { name: Some("Studio Display".to_string()), sample_rate: 48_000, channels: 2 });

        let events = logs.events();
        let [started, opened] = events.as_slice() else { panic!("{events:#?}") };
        assert_eq!(opened["event"], "dictation.microphone_opened");
        let fields = &opened["fields"];
        assert_eq!(fields["recording_id"], started["fields"]["recording_id"]);
        assert_eq!(fields["device"], "Studio Display");
        assert_eq!((&fields["sample_rate"], &fields["channels"]), (&48_000.into(), &2.into()));
    }
}
