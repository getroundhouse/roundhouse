//! Converting audio between the device's rate and its caller's: the microphone's to one channel at the rate its caller
//! asks for, such as the 16 kHz dictation's model takes, and what's played from its caller's rate to the speakers'.
//!
//! The device is heard and played at its own rate rather than asked for another, since on a Mac asking changes the
//! device's rate for every app using it. Each sample out is the samples in around where it falls, weighed by a windowed sinc
//! that passes what's under 87.5% of the slower rate's highest frequency and stops what's over 97.5% of it, which
//! would otherwise fold back as noise: down to 16 kHz, what's under 7 kHz is passed and what's over 7.8 kHz stopped.
//! The weights are worked out once, for 256 places between two samples in, so hearing is a few multiplications a
//! sample.

/// How many samples out's worth of samples in, on each side of where a sample out falls, it's made from. More makes
/// the step from passing to stopping narrower.
const HALF_WIDTH: usize = 48;

/// How many places between two samples in the weights are worked out for. A sample out takes the nearest, which is
/// at most 1/512 of a sample off, well under what the models hear.
const PLACES: usize = 256;

/// Where the filter is half way from passing to stopping, as a share of the slower rate's highest frequency: 7.36 kHz
/// down to 16 kHz, with what's under 7 kHz passed whole and what's over 7.8 kHz stopped.
const PASSED: f64 = 0.92;

/// Converts audio as it comes, a buffer at a time, keeping what the next samples out still need. The microphone's
/// audio is converted with it, and so can a recording's, such as the speech eval's, to be heard as the microphone's is.
pub struct Resampler {
    channels: u16,
    /// How many samples in each sample out moves on by.
    step: f64,
    /// How many samples in, on each side of where a sample out falls, it's made from: `HALF_WIDTH` samples out's worth.
    half: usize,
    /// `PLACES` rows of `2 * half` weights, for where a sample out falls between two samples in.
    weights: Vec<f32>,
    /// The audio in, mixed to one channel, from the first sample a sample out still needs.
    kept: Vec<f32>,
    /// Where in `kept` the next sample out falls.
    next: f64,
}

impl Resampler {
    /// Converts audio in at `rate` in `channels`, to one channel at `to`.
    pub fn new(rate: u32, channels: u16, to: u32) -> Self {
        let step = f64::from(rate) / f64::from(to);
        // In cycles a sample in: half of the slower rate's, less the room to stop.
        let cutoff = 0.5 * PASSED * (1.0 / step).min(1.0);
        let half = HALF_WIDTH * whole(step.ceil()).max(1);
        let weights = (0..PLACES)
            .flat_map(|place| {
                let between = real(place) / real(PLACES);
                (0..2 * half).map(move |tap| weight(between + real(half - 1) - real(tap), cutoff, half))
            })
            .collect();
        // Silence before the first sample, so the first samples out have something on both sides.
        let kept = vec![0.0; half];
        Self { channels: channels.max(1), step, half, weights, kept, next: real(half) }
    }

    /// Takes `interleaved` audio at the rate and channels this was made for, and gives what's ready at the rate out.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let share = 1.0 / f32::from(self.channels);
        let mixed = interleaved.chunks(usize::from(self.channels)).map(|frame| frame.iter().sum::<f32>() * share);
        self.kept.extend(mixed);
        let mut out = Vec::new();
        // The nearest place can be the next sample in, so that one has to be there too.
        while whole(self.next) + self.half + 1 < self.kept.len() {
            out.push(self.sample_at(self.next));
            self.next += self.step;
        }
        let done = whole(self.next).saturating_sub(self.half - 1);
        self.kept.drain(..done);
        self.next -= real(done);
        out
    }

    /// What's kept back of the audio pushed so far, which the samples out near its end need what follows it to make,
    /// as if silence followed. What's pushed after it follows that silence, a few milliseconds of it.
    pub fn finish(&mut self) -> Vec<f32> {
        self.push(&vec![0.0; (self.half + 1) * usize::from(self.channels)])
    }

    /// The sample out that falls at `at` in `kept`.
    fn sample_at(&self, at: f64) -> f32 {
        let nearest = whole(at * real(PLACES) + 0.5);
        let (sample, place) = (nearest / PLACES, nearest % PLACES);
        let weights = &self.weights[place * 2 * self.half..(place + 1) * 2 * self.half];
        let around = &self.kept[sample + 1 - self.half..=sample + self.half];
        weights.iter().zip(around).map(|(w, x)| w * x).sum()
    }
}

/// The weight of a sample in `distance` samples from where a sample out falls: a sinc that passes up to `cutoff`
/// cycles a sample, under a Blackman window that ends `half` samples out.
fn weight(distance: f64, cutoff: f64, half: usize) -> f32 {
    use std::f64::consts::PI;
    let width = real(half);
    if distance.abs() >= width {
        return 0.0;
    }
    let x = 2.0 * cutoff * distance;
    let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
    let phase = PI * (distance / width + 1.0);
    let window = 0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos();
    narrow(2.0 * cutoff * sinc * window)
}

#[expect(clippy::cast_precision_loss, reason = "a count of samples is far below 2^52, which an f64 holds exactly")]
fn real(n: usize) -> f64 {
    n as f64
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a place in a buffer, which is never below zero, and whose whole part is an index into it"
)]
fn whole(at: f64) -> usize {
    at as usize
}

#[expect(clippy::cast_possible_truncation, reason = "a weight needs no more precision than the samples it weighs")]
fn narrow(weight: f64) -> f32 {
    weight as f32
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::{HALF_WIDTH, Resampler};

    /// The rate out the tests convert to, dictation's.
    const SAMPLE_RATE: u32 = 16_000;

    /// `seconds` of a sine at `hz` and `level`, at `rate`, on each of `channels`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a few seconds of samples, and a sample's value"
    )]
    fn tone(hz: f64, level: f32, rate: u32, channels: u16, seconds: f64) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .flat_map(|i| {
                let v = level * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin() as f32;
                std::iter::repeat_n(v, usize::from(channels))
            })
            .collect()
    }

    /// `audio` through a resampler from `rate`, given in buffers of 10 ms as a microphone gives them, without the
    /// first and last tenth of a second, where it starts and stops.
    fn resampled(audio: &[f32], rate: u32, channels: u16) -> Vec<f32> {
        let mut resampler = Resampler::new(rate, channels, SAMPLE_RATE);
        let buffer = (rate / 100) as usize * usize::from(channels);
        let out: Vec<f32> = audio.chunks(buffer).flat_map(|chunk| resampler.push(chunk)).collect();
        let tenth = (SAMPLE_RATE / 10) as usize;
        out[tenth..out.len() - tenth].to_vec()
    }

    #[expect(clippy::cast_precision_loss, reason = "a second of samples")]
    fn rms(audio: &[f32]) -> f32 {
        (audio.iter().map(|x| x * x).sum::<f32>() / audio.len() as f32).sqrt()
    }

    /// How many times a second `audio`, at `rate`, crosses zero going up.
    #[expect(clippy::cast_precision_loss, reason = "a second of samples")]
    fn frequency(audio: &[f32], rate: u32) -> f64 {
        let ups = audio.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        ups as f64 * f64::from(rate) / audio.len() as f64
    }

    #[test]
    fn speech_frequencies_keep_their_pitch_and_level_from_every_common_rate() {
        for rate in [16_000, 22_050, 44_100, 48_000, 96_000] {
            for hz in [200.0, 1000.0, 6000.0] {
                let out = resampled(&tone(hz, 0.5, rate, 1, 1.0), rate, 1);

                let heard = frequency(&out, SAMPLE_RATE);
                assert!((heard - hz).abs() < hz * 0.01, "{hz} Hz from {rate} came out at {heard}");
                assert!((rms(&out) - 0.5 / 2f32.sqrt()).abs() < 0.01, "{hz} Hz from {rate} at {}", rms(&out));
            }
        }
    }

    /// Phonon's 24 kHz, up to the rates speakers play at, keeps a voice's pitch and level.
    #[test]
    fn a_voice_keeps_its_pitch_and_level_up_to_every_common_rate() {
        for to in [44_100, 48_000, 96_000] {
            for hz in [200.0, 1000.0, 6000.0, 10_000.0] {
                let mut resampler = Resampler::new(24_000, 1, to);
                let out = resampler.push(&tone(hz, 0.5, 24_000, 1, 1.0));
                let tenth = (to / 10) as usize;
                let out = &out[tenth..out.len() - tenth];
                let heard = frequency(out, to);

                assert!((heard - hz).abs() < hz * 0.01, "{hz} Hz up to {to} came out at {heard}");
                assert!((rms(out) - 0.5 / 2f32.sqrt()).abs() < 0.01, "{hz} Hz up to {to} at {}", rms(out));
            }
        }
    }

    /// Finishing gives the rest of what was pushed, so all of it is played, and nothing comes twice.
    #[test]
    fn finishing_gives_the_rest_of_what_was_pushed() {
        for (rate, to) in [(24_000, 48_000), (24_000, 44_100), (48_000, SAMPLE_RATE)] {
            let mut resampler = Resampler::new(rate, 1, to);
            let pushed = resampler.push(&tone(440.0, 0.5, rate, 1, 1.0)).len();
            let rest = resampler.finish().len();

            let expected = to as usize;
            assert!(pushed < expected, "some is kept back from {rate} to {to}");
            assert!((pushed + rest).abs_diff(expected) <= 2, "{pushed} and {rest} from {rate} to {to}, for {expected}");
            let after = resampler.push(&tone(440.0, 0.5, rate, 1, 0.1)).len() + resampler.finish().len();
            // A tenth of a second, after the silence finishing put before it: as many samples in as the filter is
            // wide on each side, which is wider going down.
            let half = HALF_WIDTH * rate.div_ceil(to) as usize;
            let silence = (half + 1) * to as usize / rate as usize + 2;
            assert!(
                (expected / 10..=expected / 10 + silence).contains(&after),
                "{after} for a tenth of a second from {rate} to {to}: what was finished doesn't come again"
            );
        }
    }

    #[test]
    fn a_second_of_audio_gives_a_second_at_sixteen_khz() {
        for rate in [44_100, 48_000] {
            let mut resampler = Resampler::new(rate, 1, SAMPLE_RATE);

            let out: usize = tone(440.0, 0.5, rate, 1, 3.0).chunks(441).map(|chunk| resampler.push(chunk).len()).sum();

            let expected = 3 * SAMPLE_RATE as usize;
            assert!(out.abs_diff(expected) <= 64, "{out} samples out from {rate}, for {expected}");
        }
    }

    #[test]
    fn what_is_above_eight_khz_is_stopped_rather_than_folded_back() {
        for rate in [44_100, 48_000] {
            for hz in [9000.0, 12_000.0, 18_000.0] {
                let out = resampled(&tone(hz, 0.5, rate, 1, 1.0), rate, 1);

                assert!(rms(&out) < 0.002, "{hz} Hz from {rate} came through at {}", rms(&out));
            }
        }
    }

    proptest! {
        #![proptest_config(Config {
            rng_seed: RngSeed::Fixed(1),
            failure_persistence: None,
            cases: 48,
            ..Config::default()
        })]

        #[test]
        fn any_rate_and_buffers_give_the_audio_at_sixteen_khz_however_it_is_split(
            rate in 8_000u32..=192_000,
            channels in 1u16..=4,
            audio in prop::collection::vec(-1f32..=1.0, 0..20_000),
            buffers in prop::collection::vec(1usize..2000, 1..16),
        ) {
            let channels_in = usize::from(channels);
            let audio = &audio[..audio.len() / channels_in * channels_in];
            let mut split = Resampler::new(rate, channels, SAMPLE_RATE);
            let mut out = Vec::new();
            let mut rest = audio;
            // Whole frames at a time, as a microphone gives them, of each size in turn.
            for size in buffers.iter().cycle() {
                if rest.is_empty() {
                    break;
                }
                let (buffer, after) = rest.split_at((size * channels_in).min(rest.len()));
                out.extend(split.push(buffer));
                rest = after;
            }
            let whole = Resampler::new(rate, channels, SAMPLE_RATE).push(audio);

            let frames = audio.len() / channels_in;
            let expected = frames * SAMPLE_RATE as usize / rate as usize;
            // What's still to come is what the last samples out need after the audio so far.
            let held = HALF_WIDTH * 2 + 2;
            prop_assert!(out.len() <= expected + 1 && out.len() + held >= expected, "{} for {expected}", out.len());
            prop_assert!(out.iter().all(|x| x.abs() <= 2.0), "the filter's ripple stays small");
            prop_assert!(out.len().abs_diff(whole.len()) <= 1);
            for (a, b) in out.iter().zip(&whole) {
                prop_assert!((a - b).abs() < 1e-4, "{a} split, {b} whole");
            }
        }
    }

    #[test]
    fn channels_are_mixed_to_one() {
        let mut stereo = tone(1000.0, 0.5, 48_000, 2, 1.0);
        // The right channel silent: one channel's share of the mix is half.
        stereo.iter_mut().skip(1).step_by(2).for_each(|x| *x = 0.0);

        let out = resampled(&stereo, 48_000, 2);

        assert!((rms(&out) - 0.25 / 2f32.sqrt()).abs() < 0.01, "{}", rms(&out));
    }
}
