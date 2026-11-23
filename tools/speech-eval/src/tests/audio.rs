//! Decoding recordings to what the model hears, from synthetic tones of 440 Hz at half of full scale, a quarter of a
//! second long.

use dictation::SAMPLE_RATE;

use crate::audio::decode;

/// A quarter of a second, at the model's rate.
const QUARTER: usize = SAMPLE_RATE as usize / 4;

fn loudest(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |loudest, sample| loudest.max(sample.abs()))
}

/// A WAV file of 16-bit samples, made here.
fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(u32::try_from(36 + data.len()).unwrap().to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend((channels * 2).to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
    bytes.extend(data);
    bytes
}

#[test]
fn a_48_khz_flac_of_24_bit_samples_is_heard_at_16_khz() {
    let samples = decode(include_bytes!("audio/tone-48k-24-bit.flac")).unwrap();

    assert!(samples.len().abs_diff(QUARTER) < 10, "{} samples", samples.len());
    assert!((loudest(&samples) - 0.5).abs() < 0.02, "{}", loudest(&samples));
}

#[test]
fn a_flac_in_two_channels_is_heard_in_one() {
    let samples = decode(include_bytes!("audio/tone-16k-stereo.flac")).unwrap();

    assert!(samples.len().abs_diff(QUARTER) < 10, "{} samples", samples.len());
    assert!((loudest(&samples) - 0.5).abs() < 0.02, "{}", loudest(&samples));
}

#[test]
fn a_16_khz_wav_is_heard_as_it_is() {
    let samples = decode(&wav(SAMPLE_RATE, 1, &[0, 16_384, -16_384, i16::MIN])).unwrap();

    assert_eq!(samples, [0.0, 0.5, -0.5, -1.0]);
}

#[test]
fn neither_flac_nor_wav_is_refused() {
    assert!(decode(b"ID3 an MP3").is_err());
}
