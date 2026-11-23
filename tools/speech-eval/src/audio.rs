//! A recording's file, as the samples the model hears: one channel at 16 kHz. Tech-Sentences has 48 kHz FLAC and
//! 16 kHz WAV, and `LibriSpeech` 16 kHz FLAC. Audio at another rate is converted as the microphone's is, so it reaches
//! the model as a recording in the app does.

use anyhow::{Context as _, bail, ensure};
use dictation::SAMPLE_RATE;
use sound::Resampler;

/// The samples of a FLAC or WAV file, at [`SAMPLE_RATE`] in one channel.
pub(crate) fn decode(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    let (interleaved, rate, channels) = match bytes.get(..4) {
        Some(b"fLaC") => flac(bytes)?,
        Some(b"RIFF") => wav(bytes)?,
        _ => bail!("neither a FLAC nor a WAV file"),
    };
    if rate == SAMPLE_RATE && channels == 1 {
        return Ok(interleaved);
    }
    let mut resampler = Resampler::new(rate, channels, SAMPLE_RATE);
    let mut samples = resampler.push(&interleaved);
    samples.extend(resampler.finish());
    Ok(samples)
}

/// Its samples, interleaved, with its rate and how many channels it has.
type Decoded = (Vec<f32>, u32, u16);

fn flac(bytes: &[u8]) -> anyhow::Result<Decoded> {
    let mut reader = claxon::FlacReader::new(bytes).context("not a FLAC file")?;
    let info = reader.streaminfo();
    let scale = full_scale(info.bits_per_sample)?;
    let channels = u16::try_from(info.channels).context("too many channels")?;
    let samples = reader.samples().map(|sample| Ok(to_float(sample?, scale))).collect::<Result<_, claxon::Error>>();
    Ok((samples.context("a FLAC file that doesn't decode")?, info.sample_rate, channels))
}

fn wav(bytes: &[u8]) -> anyhow::Result<Decoded> {
    ensure!(bytes.get(8..12) == Some(b"WAVE"), "a RIFF file that isn't WAV");
    let mut at = 12;
    let mut format = None;
    while let Some(header) = bytes.get(at..at + 8) {
        let (id, size) = header.split_at(4);
        let size = usize::try_from(u32::from_le_bytes(size.try_into()?))?;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        match id {
            b"fmt " => {
                ensure!(body.len() >= 16, "a WAV file whose format is cut short");
                let pcm = u16::from_le_bytes([body[0], body[1]]);
                ensure!(pcm == 1, "a WAV file that isn't PCM");
                let channels = u16::from_le_bytes([body[2], body[3]]);
                let rate = u32::from_le_bytes(body[4..8].try_into()?);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                format = Some((channels, rate, bits));
            }
            b"data" => {
                let Some((channels, rate, bits)) = format else { bail!("a WAV file with its audio before its format") };
                // 8-bit WAV is unsigned, unlike the rest.
                ensure!(bits >= 16, "a WAV file of {bits}-bit samples");
                let scale = full_scale(u32::from(bits))?;
                let width = usize::from(bits / 8);
                let samples = body.chunks_exact(width).map(|sample| to_float(little_endian(sample), scale)).collect();
                return Ok((samples, rate, channels));
            }
            _ => {}
        }
        at += 8 + size + size % 2;
    }
    bail!("a WAV file with no audio")
}

/// A sample of 1 to 4 bytes, little-endian and signed, as WAV keeps it.
fn little_endian(sample: &[u8]) -> i32 {
    let mut word = [0; 4];
    word[4 - sample.len()..].copy_from_slice(sample);
    // Shifted back down, so the sign is the sample's.
    i32::from_le_bytes(word) >> (8 * (4 - sample.len()))
}

/// The value a sample's loudest is, at `bits` bits.
fn full_scale(bits: u32) -> anyhow::Result<f32> {
    ensure!((8..=32).contains(&bits) && bits.is_multiple_of(8), "samples of {bits} bits");
    Ok(2f32.powi(i32::try_from(bits)? - 1))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a sample needs no more precision than a float's, which the model takes"
)]
fn to_float(sample: i32, full_scale: f32) -> f32 {
    sample as f32 / full_scale
}
