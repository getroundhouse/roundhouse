//! Videos for other crates' tests to play, the same ones this crate's own tests decode, made as
//! `tests/decoding/main.rs` says.

/// Two seconds of a 64 × 36 VP8 picture, ten frames a second, with a keyframe every half second.
pub const TWO_SECONDS: &[u8] = include_bytes!("../tests/decoding/fixtures/vp8-keyframes.webm");
/// A second of a 64 × 36 VP8 picture, written as it was made, as to a pipe, so it doesn't say how long it is.
pub const UNKNOWN_LENGTH: &[u8] = include_bytes!("../tests/decoding/fixtures/unknown-length.webm");
/// A WebM in AV1, which the app doesn't play.
pub const AV1: &[u8] = include_bytes!("../tests/decoding/fixtures/av1.webm");
/// A WebM in VP9's 10 bits, which opens and whose frames don't decode, so the app doesn't play it either.
pub const TEN_BIT: &[u8] = include_bytes!("../tests/decoding/fixtures/vp9-10-bit.webm");
/// A second and a half of a 64 × 36 H.264 picture, in an MP4.
pub const MP4: &[u8] = include_bytes!("../tests/decoding/fixtures/sound-first.mp4");
/// Two seconds of the spec's screen of a workspace setting up, 800 × 500, as an agent might record the app, in VP9.
pub const RECORDING_WEBM: &[u8] = include_bytes!("../tests/decoding/fixtures/recording.webm");
/// Twelve seconds of the same, in an MP4.
pub const RECORDING_MP4: &[u8] = include_bytes!("../tests/decoding/fixtures/recording.mp4");
