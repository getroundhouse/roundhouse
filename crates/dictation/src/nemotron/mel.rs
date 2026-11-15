//! The log-mel features the model was trained on, as NVIDIA's `NeMo` makes them: a frame every 10 ms of 25 ms of audio,
//! after pre-emphasis, under a Hann window, through 128 of librosa's Slaney mel filters up to 8 kHz, as logs, each
//! filter then normalised over the window's audio. Ported from `FluidAudio`'s `AudioMelSpectrogram` and
//! `UnifiedMelExtractor`, which were checked against the model's own transcripts.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

use crate::Samples;

/// How many mel filters each frame has.
pub(super) const MELS: usize = 128;

/// How far apart frames start, in samples: 10 ms.
pub(super) const HOP: usize = 160;

/// How much audio each frame is made from, in samples: 25 ms.
const SPAN: usize = 400;

/// The transform's length, which each frame's span is centred in.
const FFT: usize = 512;

/// The transform's bins, from 0 Hz to half the sample rate.
const BINS: usize = FFT / 2 + 1;

/// How much of the sample before is taken from each sample, which lifts the high frequencies speech's consonants are in.
const PRE_EMPHASIS: f32 = 0.97;

/// Added before the log, so silence has one: 2^-24, as `NeMo` adds.
const LOG_GUARD: f32 = 5.960_464_5e-8;

/// Added to each filter's spread, so a filter whose level never changes isn't divided by nothing.
const SPREAD_GUARD: f32 = 1e-5;

/// Makes the features of windows of audio. Each call is independent, so one serves every window.
pub(super) struct Mel {
    hann: Vec<f32>,
    filters: Vec<Filter>,
    fft: Arc<dyn RealToComplex<f32>>,
}

/// The features of one window, and how many of its frames are audio rather than the padding after it.
pub(super) struct Features {
    /// Filter-major: `MELS` rows of `frames` values.
    pub(super) values: Vec<f32>,
    pub(super) frames: usize,
    pub(super) audio_frames: usize,
}

impl Mel {
    pub(super) fn new() -> Self {
        // Symmetric, as `NeMo`'s `torch.hann_window(periodic=False)` is.
        let hann = (0..SPAN).map(|i| 0.5 * (1.0 - (std::f32::consts::TAU * real(i) / real(SPAN - 1)).cos())).collect();
        Self { hann, filters: filters(), fft: RealFftPlanner::<f32>::new().plan_fft_forward(FFT) }
    }

    /// The features of `window`, whose first `audio` is audio and the rest padding, as the model's encoders take them.
    /// A frame for each `HOP` of the window and one more, since frames are centred on where they start.
    pub(super) fn features(&self, window: &[f32], audio: Samples) -> Features {
        let frames = window.len() / HOP + 1;
        let logs = self.logs(window, frames);
        // `NeMo` counts no frame for the last partial hop, so the window's last frame is always padding.
        let audio_frames = (audio.0 / HOP).min(frames);
        let mut values = vec![0.0; MELS * frames];
        if audio_frames > 0 {
            let spread_over = real(audio_frames.saturating_sub(1).max(1));
            for mel in 0..MELS {
                let level = |frame: usize| logs[frame * MELS + mel];
                let mean = (0..audio_frames).map(level).sum::<f32>() / real(audio_frames);
                let variance = (0..audio_frames).map(|frame| (level(frame) - mean).powi(2)).sum::<f32>() / spread_over;
                let spread = variance.sqrt() + SPREAD_GUARD;
                for frame in 0..audio_frames {
                    values[mel * frames + frame] = (level(frame) - mean) / spread;
                }
            }
        }
        Features { values, frames, audio_frames }
    }

    /// The log of each filter's power in each frame, frame-major.
    fn logs(&self, window: &[f32], frames: usize) -> Vec<f32> {
        // Pre-emphasised, with half a transform of silence on each side, so the first and last frames are centred on
        // the window's ends.
        let pad = FFT / 2;
        let mut padded = vec![0.0; window.len() + 2 * pad];
        let mut before = 0.0;
        for (i, &sample) in window.iter().enumerate() {
            padded[pad + i] = sample - PRE_EMPHASIS * before;
            before = sample;
        }
        let inset = (FFT - SPAN) / 2;
        let mut frame = vec![0.0; FFT];
        let mut spectrum = self.fft.make_output_vec();
        let mut scratch = self.fft.make_scratch_vec();
        let mut power = vec![0.0; BINS];
        let mut logs = vec![0.0; frames * MELS];
        for f in 0..frames {
            frame.fill(0.0);
            let start = f * HOP + inset;
            let taken = SPAN.min(padded.len().saturating_sub(start));
            for i in 0..taken {
                frame[inset + i] = padded[start + i] * self.hann[i];
            }
            // It fails only for buffers of other lengths than the plan's, and these are made from the plan.
            let _ = self.fft.process_with_scratch(&mut frame, &mut spectrum, &mut scratch);
            for (p, bin) in power.iter_mut().zip(&spectrum) {
                *p = bin.norm_sqr();
            }
            for (mel, filter) in self.filters.iter().enumerate() {
                logs[f * MELS + mel] = (filter.sum(&power) + LOG_GUARD).ln();
            }
        }
        logs
    }
}

/// One mel filter: its weights for the bins it covers, from `first`. It has none for the rest, so they're left out.
struct Filter {
    first: usize,
    weights: Vec<f32>,
}

impl Filter {
    /// How much of `power`, a frame's power in each bin, the filter takes.
    fn sum(&self, power: &[f32]) -> f32 {
        self.weights.iter().zip(&power[self.first..]).map(|(w, p)| w * p).sum()
    }
}

/// The filters, each cut to the bins it has weights for, since most of a filter's weights are 0.
fn filters() -> Vec<Filter> {
    weights()
        .into_iter()
        .map(|weights| {
            let covered = weights.iter().position(|&w| w != 0.0).zip(weights.iter().rposition(|&w| w != 0.0));
            match covered {
                Some((first, last)) => Filter { first, weights: weights[first..=last].to_vec() },
                None => Filter { first: 0, weights: Vec::new() },
            }
        })
        .collect()
}

/// librosa's Slaney mel filters from 0 to 8 kHz, each normalised by its width, worked out in `f32` as `FluidAudio` does:
/// a weight for each bin, for each filter.
fn weights() -> Vec<Vec<f32>> {
    let hz_per_mel = 200.0_f32 / 3.0;
    let log_from_hz = 1000.0_f32;
    let log_from_mel = log_from_hz / hz_per_mel;
    let log_step = 6.4_f32.ln() / 27.0;
    let to_mel = |hz: f32| {
        if hz >= log_from_hz { log_from_mel + (hz / log_from_hz).ln() / log_step } else { hz / hz_per_mel }
    };
    let to_hz = |mel: f32| {
        if mel >= log_from_mel { log_from_hz * (log_step * (mel - log_from_mel)).exp() } else { hz_per_mel * mel }
    };
    let (lowest, highest) = (to_mel(0.0), to_mel(8000.0));
    let edges: Vec<f32> =
        (0..MELS + 2).map(|i| to_hz(lowest + real(i) * (highest - lowest) / real(MELS + 1))).collect();
    let hz = |bin: usize| real(bin) * 16_000.0 / real(FFT);
    (0..MELS)
        .map(|mel| {
            let (left, centre, right) = (edges[mel], edges[mel + 1], edges[mel + 2]);
            let height = 2.0 / (right - left);
            let weight = |bin: usize| {
                let f = hz(bin);
                if f >= left && f < centre {
                    height * (f - left) / (centre - left)
                } else if f >= centre && f <= right {
                    height * (right - f) / (right - centre)
                } else {
                    0.0
                }
            };
            (0..BINS).map(weight).collect()
        })
        .collect()
}

#[expect(clippy::cast_precision_loss, reason = "counts of samples and frames in a window, which an f32 holds exactly")]
fn real(n: usize) -> f32 {
    n as f32
}

#[cfg(test)]
mod tests {
    use super::{BINS, HOP, MELS, Mel};
    use crate::Samples;

    fn tone(hz: f32, samples: usize) -> Vec<f32> {
        (0..samples).map(|i| 0.5 * (std::f32::consts::TAU * hz * super::real(i) / 16_000.0).sin()).collect()
    }

    #[test]
    fn a_window_has_a_frame_for_each_hop_and_one_more_and_counts_only_its_audio() {
        let mel = Mel::new();
        let mut window = tone(440.0, 16_000);
        window.resize(16_000 * 2, 0.0);

        let features = mel.features(&window, Samples(16_000));

        assert_eq!(features.frames, 2 * 16_000 / HOP + 1);
        assert_eq!(features.audio_frames, 16_000 / HOP);
        assert_eq!(features.values.len(), MELS * features.frames);
        // The padding after the audio is left at zero, which is the normalised mean.
        let padding = (0..MELS).flat_map(|mel| (features.audio_frames..features.frames).map(move |f| (mel, f)));
        assert!(padding.into_iter().all(|(mel, f)| features.values[mel * features.frames + f] == 0.0));
    }

    #[test]
    fn each_filter_is_normalised_over_the_audio() {
        let mel = Mel::new();
        let window = tone(440.0, 16_000);

        let features = mel.features(&window, Samples(window.len()));

        for mel in 0..MELS {
            let row = &features.values[mel * features.frames..][..features.audio_frames];
            let mean = row.iter().sum::<f32>() / super::real(row.len());
            let variance = row.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / super::real(row.len() - 1);
            assert!(mean.abs() < 1e-3, "filter {mel} has mean {mean}");
            // Divided by its spread, over one fewer than its frames, as `NeMo` divides, so it's 1 unless the filter's
            // level hardly changes, when the guard added to the spread keeps it lower.
            assert!(variance <= 1.0 + 1e-3, "filter {mel} has variance {variance}");
        }
    }

    #[test]
    fn each_filter_takes_just_what_its_weight_for_every_bin_would() {
        let filters = super::filters();
        let weights = super::weights();
        let mut seed = 1_u64;
        for _ in 0..100 {
            let power: Vec<f32> = (0..BINS)
                .map(|_| {
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    f32::from(u16::try_from(seed >> 48).unwrap())
                })
                .collect();
            for (mel, (filter, weights)) in filters.iter().zip(&weights).enumerate() {
                let every_bin: f32 = weights.iter().zip(&power).map(|(w, p)| w * p).sum();
                // To the bit, since all it leaves out is adding nothing.
                assert_eq!(filter.sum(&power).to_bits(), every_bin.to_bits(), "filter {mel}");
            }
        }
    }

    #[test]
    fn a_tone_is_loudest_in_the_filter_around_its_pitch() {
        let mel = Mel::new();
        let window = tone(1000.0, 8_000);

        let logs = mel.logs(&window, window.len() / HOP + 1);

        let middle = &logs[(window.len() / HOP / 2) * MELS..][..MELS];
        let loudest = (0..MELS).max_by(|&a, &b| middle[a].total_cmp(&middle[b])).unwrap();
        // 1 kHz is 15 of the 45.25 mels up to 8 kHz, and the filters are spread evenly over them, 0.35 apart, so it's
        // at the centre of the 42nd.
        assert!((40..=43).contains(&loudest), "loudest in filter {loudest}");
    }
}
