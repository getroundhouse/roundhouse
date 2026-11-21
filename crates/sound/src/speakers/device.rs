//! The system's default output device, through `cpal`: Core Audio on a Mac, WASAPI on Windows and ALSA on Linux.

use std::sync::Arc;

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use cpal::{ErrorKind, SampleFormat};
use parking_lot::Mutex;

use super::SpeakersError;
use super::queue::Queue;
use crate::resample::Resampler;

/// What the speakers say as they play, on [`Speakers::played`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Played {
    /// Everything given before this mark has gone to the device, a moment before it's heard.
    Reached(u64),
    /// The device stopped working, such as when it was unplugged, so nothing more is played.
    Failed(SpeakersError),
}

/// The system's default output device, playing what it's given as soon as it can, and silence otherwise, until this is
/// dropped. It plays at the device's own rate, since on a Mac asking for another changes it for every app using it, so
/// what's given is converted to it first. It stays on the thread that opened it, since not every platform's stream
/// can move to another.
pub struct Speakers {
    _stream: cpal::Stream,
    queue: Arc<Mutex<Queue>>,
    /// From the rate audio is given at to the device's.
    resampler: Resampler,
    rate: u32,
    device_rate: u32,
    played: flume::Receiver<Played>,
}

impl Speakers {
    /// Opens the default output device, to play audio given in one channel at `rate`.
    pub fn open(rate: u32) -> Result<Self, SpeakersError> {
        let device = cpal::default_host().default_output_device().ok_or(SpeakersError::Missing)?;
        let config = device.default_output_config().map_err(|e| failure(&e))?;
        let channels = usize::from(config.channels());
        let queue = Arc::new(Mutex::new(Queue::default()));
        let (tell, played) = flume::unbounded();
        let failed = tell.clone();
        let on_error = move |e: cpal::Error| {
            // A few samples late, or the system's output changed to another device and the stream was moved to it,
            // converting to its rate, as a Mac does. Either way the stream carries on, and anything else ends it.
            if e.kind() != ErrorKind::Xrun && e.kind() != ErrorKind::DeviceChanged {
                let _ = failed.send(Played::Failed(failure(&e)));
            }
        };
        let playing = queue.clone();
        let mut reached = move |mark| {
            let _ = tell.send(Played::Reached(mark));
        };
        let format = config.sample_format();
        let stream = if format == SampleFormat::F32 {
            device.build_output_stream::<f32, _, _>(
                config.config(),
                move |out, _| playing.lock().fill(out, channels, |sample| sample, &mut reached),
                on_error,
                None,
            )
        } else if format == SampleFormat::I16 {
            device.build_output_stream::<i16, _, _>(
                config.config(),
                move |out, _| playing.lock().fill(out, channels, to_i16, &mut reached),
                on_error,
                None,
            )
        } else {
            return Err(SpeakersError::Failed(format!("it takes its samples as {format}")));
        }
        .map_err(|e| failure(&e))?;
        stream.play().map_err(|e| failure(&e))?;
        let device_rate = config.sample_rate();
        Ok(Self { _stream: stream, queue, resampler: Resampler::new(rate, 1, device_rate), rate, device_rate, played })
    }

    /// Each mark as it's reached, and the device failing, which closing the speakers leaves behind with them.
    pub fn played(&self) -> &flume::Receiver<Played> {
        &self.played
    }

    /// Plays `audio`, in one channel at the rate this was opened for, after what's still to be played. A little of it
    /// is kept back until what follows it comes, or [`Self::mark`] ends it.
    pub fn play(&mut self, audio: &[f32]) {
        let converted = self.resampler.push(audio);
        self.queue.lock().give(&converted);
    }

    /// Plays what [`Self::play`] kept back, then puts `mark` after it, so it's [`Played::Reached`] once everything given
    /// so far has gone to the device.
    pub fn mark(&mut self, mark: u64) {
        let rest = self.resampler.finish();
        let mut queue = self.queue.lock();
        queue.give(&rest);
        queue.mark(mark);
    }

    /// Stops at once: drops what's still to be played, and its marks, which are never sent.
    pub fn stop(&mut self) {
        self.queue.lock().clear();
        self.resampler = Resampler::new(self.rate, 1, self.device_rate);
    }
}

/// A sample as a 16-bit device takes it.
#[expect(clippy::cast_possible_truncation, reason = "clamped to what 16 bits hold first")]
fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

/// What `cpal`'s error means here.
fn failure(e: &cpal::Error) -> SpeakersError {
    // Compared rather than matched, since `cpal` can add kinds, and every other is the device failing.
    if e.kind() == ErrorKind::DeviceNotAvailable {
        SpeakersError::Missing
    } else {
        SpeakersError::Failed(e.to_string())
    }
}
