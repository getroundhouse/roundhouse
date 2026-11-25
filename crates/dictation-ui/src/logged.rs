//! What's said of a recording: to the app's log, which never has what was said or hinted, and to whatever keeps it, if
//! the person chose to.

use std::fmt;
use std::time::{Duration, Instant};

use dictation::{DictationError, Ending};
use sound::InputDevice;
use uuid::Uuid;

use crate::Place;
use crate::keeping::{How, Said};

/// One recording, with an event in the log as it starts and another as it ends, however it ends, since this goes with
/// the recording. A recording that's kept has the same said of it, with what was heard.
pub(crate) struct Logged {
    id: RecordingId,
    started: Instant,
    stopped: Option<Instant>,
    how: How,
    error: Option<String>,
    keeping: Option<flume::Sender<(Duration, Said)>>,
}

/// Which recording an event is about.
#[derive(Clone, Copy)]
struct RecordingId(Uuid);

impl fmt::Display for RecordingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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
        let started = Instant::now();
        Self { id, started, stopped: None, how: How::FieldGone, error: None, keeping: None }
    }

    /// Has what keeps the recording told what's said of it from now on, and gives what tells it, and the recording's
    /// ID.
    pub(crate) fn keep(&mut self) -> (String, flume::Receiver<(Duration, Said)>) {
        let (keeping, said) = flume::unbounded();
        self.keeping = Some(keeping);
        (self.id.to_string(), said)
    }

    fn tell(&self, said: Said) {
        if let Some(keeping) = &self.keeping {
            let _ = keeping.send((self.started.elapsed(), said));
        }
    }

    pub(crate) fn live(&self, text: &str) {
        self.tell(Said::Live(text.to_string()));
    }

    pub(crate) fn final_text(&self, text: &str) {
        self.tell(Said::Final(text.to_string()));
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
        self.tell(Said::Opened(device.clone()));
    }

    /// The last words are being heard, until it ends.
    pub(crate) fn stopped(&mut self) {
        self.stopped = Some(Instant::now());
        self.tell(Said::Stopped);
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
        self.error = Some(error.to_string());
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
        let error = self.error.take();
        self.tell(Said::Ended { how: self.how, error });
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
