use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::io::Cursor;
use std::time::Duration;
use video::WebmVideo;

use super::{THREADS, UNKNOWN_LENGTH, VP8_KEYFRAMES, hash};

/// Each frame of a file, as decoding it from the start gives it: its time and its hash.
fn from_the_start(bytes: &[u8]) -> Vec<(Duration, u64)> {
    let mut video = WebmVideo::open(Cursor::new(bytes), THREADS).unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = video.next_frame().unwrap() {
        frames.push((frame.time, hash(&frame.picture)));
    }
    frames
}

/// A WebM written as it's made, as to a pipe, has no length and no index, since its writer can't go back for them, so
/// it says none, and a seek finds its keyframe by reading.
#[test]
fn a_video_that_doesn_t_say_how_long_it_is_plays_and_seeks() {
    let all = from_the_start(UNKNOWN_LENGTH);
    let mut video = WebmVideo::open(Cursor::new(UNKNOWN_LENGTH), THREADS).unwrap();

    video.seek(Duration::from_millis(700)).unwrap();

    assert_eq!(video.duration(), None);
    assert_eq!(all.len(), 10, "a second, ten frames a second");
    let next = video.next_frame().unwrap().map(|frame| (frame.time, hash(&frame.picture)));
    assert_eq!(next, Some(all[7]));
}

/// The same file with each unit of its timestamps two seconds, more than any frame is apart, which Matroska allows.
/// Its Info is rewritten in place, its timestamp scale a byte longer and its duration four bytes shorter, as a float
/// of four, with a Void of three after them, so nothing after it moves.
fn in_units_of_two_seconds(bytes: &[u8]) -> Vec<u8> {
    let mut scale = vec![0x2a, 0xd7, 0xb1, 0x84];
    scale.extend(2_000_000_000u32.to_be_bytes());
    let mut duration = vec![0x44, 0x89, 0x84];
    duration.extend(2000f32.to_be_bytes());
    let (from_scale, from_duration) = (b"\x2a\xd7\xb1\x83\x0f\x42\x40", b"\x44\x89\x88");
    let at = |what: &[u8]| bytes.windows(what.len()).position(|window| window == what).unwrap();
    let (scale_at, duration_at) = (at(from_scale), at(from_duration));
    let mut changed = bytes[..scale_at].to_vec();
    changed.extend(scale);
    changed.extend(&bytes[scale_at + from_scale.len()..duration_at]);
    changed.extend(duration);
    changed.extend([0xec, 0x81, 0x00]);
    changed.extend(&bytes[duration_at + 11..]);
    assert_eq!(changed.len(), bytes.len());
    changed
}

/// A seek past the end of a file whose unit is longer than a second ends, finding nothing to read.
#[test]
fn a_seek_ends_in_a_file_whose_unit_is_longer_than_a_second() {
    let mut video = WebmVideo::open(Cursor::new(in_units_of_two_seconds(VP8_KEYFRAMES)), THREADS).unwrap();

    video.seek(Duration::from_secs(1_000_000)).unwrap();

    assert_eq!(video.duration(), Some(Duration::from_secs(4000)), "2000 units of two seconds");
    assert!(video.next_frame().unwrap().is_none());
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Wherever a seek goes, back or forward, and however often, the next frames are those decoding from the start
    /// gives from that time on, decoded from the keyframe before it, or none past the end.
    #[test]
    fn a_seek_gives_the_frames_decoding_from_the_start_does_from_there(
        seeks in prop::collection::vec((0u64..2200, 1usize..4), 1..6),
    ) {
        let all = from_the_start(VP8_KEYFRAMES);
        let mut video = WebmVideo::open(Cursor::new(VP8_KEYFRAMES), THREADS).unwrap();

        for (millis, count) in seeks {
            let time = Duration::from_millis(millis);
            video.seek(time).unwrap();

            let mut read = Vec::new();
            while read.len() < count && let Some(frame) = video.next_frame().unwrap() {
                read.push((frame.time, hash(&frame.picture)));
            }
            let expected: Vec<_> = all.iter().filter(|(at, _)| *at >= time).take(count).copied().collect();
            prop_assert_eq!(read, expected, "after a seek to {:?}", time);
        }
    }
}
