#![cfg(test)]
//! Decoding the fixtures, which are ffmpeg's test pattern, made with:
//!
//! ```sh
//! O="-an -fflags +bitexact -flags:v +bitexact -map_metadata -1"
//! ffmpeg -f lavfi -i testsrc2=size=64x36:rate=10:duration=0.5 -c:v libvpx -b:v 200k $O vp8.webm
//! ffmpeg -f lavfi -i testsrc2=size=66x38:rate=10:duration=0.5 -vf crop=65:37:0:0:exact=1 -pix_fmt yuv420p \
//!   -c:v libvpx-vp9 -b:v 200k $O vp9-odd-size.webm
//! for f in yuv444p yuv422p yuv440p rgb24; do
//!   ffmpeg -f lavfi -i testsrc2=size=66x38:rate=10:duration=0.5 -vf crop=65:37:0:0:exact=1,format=$f \
//!     -c:v libvpx-vp9 -b:v 200k $O vp9-${f%p}.webm
//! done
//! mv vp9-rgb24.webm vp9-rgb.webm
//! ffmpeg -f lavfi -i testsrc2=size=64x36:rate=10:duration=0.2 -pix_fmt yuv420p10le -c:v libvpx-vp9 -b:v 200k $O \
//!   vp9-10-bit.webm
//! ffmpeg -f lavfi -i testsrc2=size=64x64:rate=10:duration=0.2 -c:v libsvtav1 $O av1.webm
//! ffmpeg -f lavfi -i sine=duration=0.2 -c:a libopus -map_metadata -1 sound-only.webm
//! ffmpeg -f lavfi -i testsrc2=size=64x36:rate=10:duration=2 -c:v libvpx -b:v 200k -g 5 -keyint_min 5 $O \
//!   vp8-keyframes.webm
//! ffmpeg -f lavfi -i testsrc2=size=64x36:rate=10:duration=1 -c:v libvpx -b:v 200k -g 5 -keyint_min 5 $O -f webm \
//!   pipe:1 > unknown-length.webm
//! ffmpeg -f lavfi -i testsrc2=size=64x36:rate=10:duration=1.5 -f lavfi -i sine=duration=1.5 -map 1:a -map 0:v \
//!   -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 16k -fflags +bitexact -flags:v +bitexact -map_metadata -1 \
//!   sound-first.mp4
//! ffmpeg -f lavfi -i testsrc2=size=48x32:rate=10:duration=2 -c:v libx264 -pix_fmt yuv420p -movflags +faststart \
//!   -fflags +bitexact -flags:v +bitexact -map_metadata -1 moov-first.mov
//! ```
//!
//! And for other crates' tests, as `video::fixtures` has them, two of the spec's screen of a workspace setting up:
//!
//! ```sh
//! S=docs/spec/screens/setting-up.png
//! ffmpeg -loop 1 -framerate 5 -i $S -t 2 -vf scale=800:500:flags=lanczos,format=yuv420p -c:v libvpx-vp9 -crf 40 \
//!   -b:v 0 -row-mt 0 $O recording.webm
//! ffmpeg -loop 1 -framerate 1 -i $S -t 12 -vf scale=800:500:flags=lanczos,format=yuv420p -c:v libx264 -crf 35 \
//!   -tune stillimage $O recording.mp4
//! ```
//!
//! Each frame's hash is ffmpeg's own decoder's, not libvpx's, of the same frame: VP8 and VP9 decode exactly, so the two
//! agree, on every platform, whichever of libvpx's paths decodes it, NEON, nasm's assembly or plain C.

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::io::Cursor;
use std::num::NonZeroU32;
use std::time::Duration;
use video::{DecodeError, OpenError, Picture, ReadError, WebmVideo};

mod iso_media;
mod parts;
mod playing;
mod seeking;

const VP8: &[u8] = include_bytes!("fixtures/vp8.webm");
/// Two seconds, with a keyframe every half second.
const VP8_KEYFRAMES: &[u8] = include_bytes!("fixtures/vp8-keyframes.webm");
const VP9_ODD_SIZE: &[u8] = include_bytes!("fixtures/vp9-odd-size.webm");
/// The same picture at full chroma, at half the width's, at half the height's, and as green, blue and red, which is
/// how VP9 stores a video made from RGB frames, such as screenshots.
const VP9_444: &[u8] = include_bytes!("fixtures/vp9-yuv444.webm");
const VP9_422: &[u8] = include_bytes!("fixtures/vp9-yuv422.webm");
const VP9_440: &[u8] = include_bytes!("fixtures/vp9-yuv440.webm");
const VP9_RGB: &[u8] = include_bytes!("fixtures/vp9-rgb.webm");
/// In VP9's profile of 10 and 12 bits, which libvpx is built without.
const VP9_TEN_BIT: &[u8] = include_bytes!("fixtures/vp9-10-bit.webm");
const AV1: &[u8] = include_bytes!("fixtures/av1.webm");
const SOUND_ONLY: &[u8] = include_bytes!("fixtures/sound-only.webm");
/// A second, with a keyframe every half second, written as it was made, with no length and no index.
const UNKNOWN_LENGTH: &[u8] = include_bytes!("fixtures/unknown-length.webm");

/// More than one, so VP9 decodes on several threads.
const THREADS: NonZeroU32 = NonZeroU32::new(2).unwrap();

/// A fixture, and what ffmpeg made of it: its size, and the hash of each frame, a tenth of a second apart.
struct Fixture {
    name: &'static str,
    bytes: &'static [u8],
    width: u32,
    height: u32,
    hashes: [u64; 5],
}

/// Each fixture of a picture that libvpx decodes, in VP8 and in each of the ways VP9 stores colour.
const FIXTURES: [Fixture; 6] = [
    Fixture {
        name: "VP8",
        bytes: VP8,
        width: 64,
        height: 36,
        hashes: [
            0x3651_05de_5d33_27ab,
            0x4ac8_7fcd_e0a5_5bbf,
            0x8c17_2cda_030e_f829,
            0x4e11_af00_d05c_6d00,
            0x5615_7690_9809_4ce0,
        ],
    },
    Fixture {
        name: "VP9 at an odd size",
        bytes: VP9_ODD_SIZE,
        width: 65,
        height: 37,
        hashes: [
            0x70f6_56c5_d980_ac9c,
            0x70f6_56c5_d980_ac9c,
            0xd702_539c_3ae4_ac12,
            0xe943_366b_74b0_5b42,
            0x4707_d8b7_b12a_c00f,
        ],
    },
    Fixture {
        name: "VP9 at full chroma",
        bytes: VP9_444,
        width: 65,
        height: 37,
        hashes: [
            0xe2fc_50ab_6a3d_ba8c,
            0x50d3_5dc0_1778_7648,
            0xbbfa_deb0_5528_8379,
            0x7a2b_c84a_25c6_dc78,
            0x5775_c11b_e5df_20aa,
        ],
    },
    Fixture {
        name: "VP9 at half the width's chroma",
        bytes: VP9_422,
        width: 65,
        height: 37,
        hashes: [
            0x0724_d84f_97ea_3cbe,
            0x0724_d84f_97ea_3cbe,
            0x82d2_9c3c_502e_93ef,
            0xc70f_f8fc_475f_1e71,
            0x877d_c730_816c_fdd0,
        ],
    },
    Fixture {
        name: "VP9 at half the height's chroma",
        bytes: VP9_440,
        width: 65,
        height: 37,
        hashes: [
            0xd91a_d398_7d6f_568e,
            0xd91a_d398_7d6f_568e,
            0x6dc3_8627_3573_b864,
            0xc10a_d17a_77f4_737a,
            0xa977_109a_da29_eaf1,
        ],
    },
    Fixture {
        name: "VP9 in green, blue and red",
        bytes: VP9_RGB,
        width: 65,
        height: 37,
        hashes: [
            0xc1b1_1815_e7b6_efc9,
            0x3517_f260_3fc4_7c4c,
            0x1fcd_f98e_eef8_a007,
            0xf780_33ec_8d77_37ec,
            0x7f05_1374_fe06_914f,
        ],
    },
];

#[test]
fn each_video_decodes_to_the_frames_ffmpeg_does_at_their_times() {
    for fixture in FIXTURES {
        let mut video = WebmVideo::open(Cursor::new(fixture.bytes), THREADS).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = video.next_frame().unwrap() {
            let picture = &frame.picture;
            frames.push((frame.time, (picture.width, picture.height), hash(picture)));
        }

        let name = fixture.name;
        assert_eq!(video.size(), (fixture.width, fixture.height), "{name}");
        assert_eq!(video.duration(), Some(Duration::from_millis(500)), "{name}");
        let size = (usize::try_from(fixture.width).unwrap(), usize::try_from(fixture.height).unwrap());
        let times = (0..).map(|tenths| Duration::from_millis(100 * tenths));
        let expected: Vec<_> = times.zip(fixture.hashes).map(|(time, hash)| (time, size, hash)).collect();
        assert_eq!(frames, expected, "{name}");
    }
}

/// Green, blue and red come out as they're stored, so each frame's BGRA is ffmpeg's, byte for byte.
#[test]
fn a_video_in_green_blue_and_red_draws_as_ffmpeg_draws_it() {
    let ffmpegs: [u64; 5] = [
        0x31da_1470_827d_4d26,
        0xce96_bdcb_b2a3_125b,
        0x0bf5_c7a1_c28d_8e46,
        0xe9c6_b11c_ea23_9897,
        0x0a9f_967e_133b_9d72,
    ];
    let mut video = WebmVideo::open(Cursor::new(VP9_RGB), THREADS).unwrap();
    let (mut rows, mut bgra, mut drawn) = (video::Rows::default(), Vec::new(), Vec::new());

    while let Some(frame) = video.next_frame().unwrap() {
        assert_eq!(frame.picture.colors.matrix, video::Matrix::Identity);
        frame.picture.to_bgra(&mut rows, &mut bgra);
        drawn.push(fnv(&bgra));
    }

    assert_eq!(drawn, ffmpegs);
}

#[test]
fn a_file_without_vp8_or_vp9_video_says_why() {
    let av1 = WebmVideo::open(Cursor::new(AV1), THREADS);
    let sound = WebmVideo::open(Cursor::new(SOUND_ONLY), THREADS);
    let picture = WebmVideo::open(Cursor::new(b"\x89PNG\r\n\x1a\n".as_slice()), THREADS);

    assert!(matches!(av1, Err(OpenError::Codec { ref codec }) if codec == "V_AV1"), "{:?}", av1.err());
    assert!(matches!(sound, Err(OpenError::NoVideo)), "{:?}", sound.err());
    assert!(matches!(picture, Err(OpenError::Read(_))), "{:?}", picture.err());
}

/// One in 10 bits opens, since its stream says only that it's VP9, and libvpx refuses its first frame.
#[test]
fn a_video_in_more_than_8_bits_opens_and_its_frames_don_t_decode() {
    let mut video = WebmVideo::open(Cursor::new(VP9_TEN_BIT), THREADS).unwrap();

    let first = video.next_frame().map(|frame| frame.map(|frame| frame.time));

    assert!(matches!(first, Err(ReadError::Decode(DecodeError::Libvpx(_)))), "{first:?}");
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// A file is what an agent wrote, so it can be anything. One with bytes changed or cut short, anywhere, reads to
    /// its end or an error, and never panics, which would take the player's thread with it.
    #[test]
    fn a_broken_file_reads_to_an_end_or_an_error(
        fixture in 0..4usize,
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
        kept in any::<prop::sample::Index>(),
    ) {
        let mut bytes = [VP8, VP9_ODD_SIZE, VP9_444, VP9_RGB][fixture].to_vec();
        for (at, byte) in changes {
            let at = at.index(bytes.len());
            bytes[at] = byte;
        }
        bytes.truncate(kept.index(bytes.len()) + 1);

        if let Ok(mut video) = WebmVideo::open(Cursor::new(bytes), THREADS) {
            // A broken frame can read as more frames than there are, so it stops at what one file could hold.
            for _ in 0..64 {
                if !matches!(video.next_frame(), Ok(Some(_))) {
                    break;
                }
            }
        }
    }
}

/// FNV-1a over the picture's Y, U and V rows, as the fixtures' hashes of ffmpeg's frames were made.
fn hash(picture: &Picture<'_>) -> u64 {
    let rows = picture.y.rows().chain(picture.u.rows()).chain(picture.v.rows());
    fnv(rows.flatten())
}

/// FNV-1a over `bytes`.
fn fnv<'a>(bytes: impl IntoIterator<Item = &'a u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}
