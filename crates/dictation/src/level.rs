//! A recording's current loudness, measured before recognition so a slow model cannot delay its visual response.

use std::sync::Arc;

use parking_lot::Mutex;

/// Normalized microphone energy, from silence to full strength. This is loudness, not recognition confidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Level(u8);

impl Level {
    pub fn strength(self) -> f32 {
        f32::from(self.0) / 100.
    }
}

/// Thirty milliseconds at 16 kHz: enough samples for a steady level, at most 34 updates per second.
const WINDOW: u16 = 480;

#[derive(Clone)]
pub(crate) struct Meter(Arc<Mutex<Measuring>>);

struct Measuring {
    power: f32,
    samples: u16,
    last: Level,
    send: flume::Sender<Level>,
    latest: flume::Receiver<Level>,
}

impl Meter {
    pub(crate) fn new() -> Self {
        let (send, latest) = flume::bounded(1);
        Self(Arc::new(Mutex::new(Measuring { power: 0., samples: 0, last: Level::default(), send, latest })))
    }

    pub(crate) fn levels(&self) -> flume::Receiver<Level> {
        self.0.lock().latest.clone()
    }

    pub(crate) fn hear(&self, audio: &[f32]) {
        let mut meter = self.0.lock();
        for &sample in audio {
            let sample = if sample.is_finite() { sample.clamp(-1., 1.) } else { 0. };
            meter.power += sample * sample;
            meter.samples += 1;
            if meter.samples == WINDOW {
                let level = normalized(meter.power / f32::from(WINDOW));
                meter.power = 0.;
                meter.samples = 0;
                if level != meter.last {
                    meter.last = level;
                    // Keep only the newest level. A busy UI must never replay a backlog of old speech.
                    let _ = meter.latest.try_recv();
                    let _ = meter.send.try_send(level);
                }
            }
        }
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped, rounded percentage fits u8")]
fn normalized(power: f32) -> Level {
    // Below -50 dB is a quiet room; -12 dB is already strong speech. A small range keeps the hairline restrained.
    let db = 10. * power.max(1e-12).log10();
    Level(((db + 50.) / 38. * 100.).clamp(0., 100.).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::{Meter, WINDOW};

    #[test]
    fn levels_follow_audio_in_fixed_windows_and_keep_only_the_newest() {
        let meter = Meter::new();
        let levels = meter.levels();
        meter.hear(&vec![0.1; usize::from(WINDOW) - 1]);
        assert!(levels.is_empty(), "partial buffers aren't a full measurement");
        meter.hear(&[0.1]);
        assert!(levels.recv().unwrap().strength() > 0.5);

        meter.hear(&vec![1.; usize::from(WINDOW)]);
        meter.hear(&vec![0.; usize::from(WINDOW)]);
        assert_eq!(levels.len(), 1, "a slow reader only sees the current level");
        assert_eq!(levels.recv().unwrap().strength(), 0.);
        meter.hear(&vec![0.; usize::from(WINDOW) * 10]);
        assert!(levels.is_empty(), "silence doesn't keep waking the UI");
    }

    #[test]
    fn invalid_samples_do_not_poison_the_following_measurements() {
        let meter = Meter::new();
        let levels = meter.levels();
        meter.hear(&vec![f32::NAN; usize::from(WINDOW)]);
        meter.hear(&vec![f32::INFINITY; usize::from(WINDOW)]);
        assert!(levels.is_empty());
        meter.hear(&vec![1.; usize::from(WINDOW)]);
        assert_eq!(levels.recv().unwrap().strength(), 1.);
    }
}
