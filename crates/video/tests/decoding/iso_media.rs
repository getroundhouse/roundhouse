//! What an MP4 or a QuickTime file says of its video, wherever its `moov` box is, and whatever its boxes say.

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::io::Cursor;
use std::time::Duration;
use video::{About, about};

/// A second and a half of a 64 × 36 picture, with its sound as the first track, and its `moov` at the end, as ffmpeg
/// writes one by default.
const SOUND_FIRST: &[u8] = include_bytes!("fixtures/sound-first.mp4");
/// Two seconds of a 48 × 32 picture, with its `moov` at the start.
const MOOV_FIRST: &[u8] = include_bytes!("fixtures/moov-first.mov");

fn read(bytes: &[u8]) -> About {
    about(&mut Cursor::new(bytes)).unwrap()
}

/// A box of `kind` around `inside`, its size in four bytes, or in eight after its kind when `long`.
fn boxed(kind: [u8; 4], inside: &[u8], long: bool) -> Vec<u8> {
    let mut boxed = Vec::new();
    if long {
        boxed.extend(1_u32.to_be_bytes());
        boxed.extend(kind);
        boxed.extend((16 + inside.len() as u64).to_be_bytes());
    } else {
        boxed.extend(u32::try_from(8 + inside.len()).unwrap().to_be_bytes());
        boxed.extend(kind);
    }
    boxed.extend(inside);
    boxed
}

/// A version 1 movie header, a duration of `duration` in `scale`ths of a second.
fn movie_header(scale: u32, duration: u64) -> Vec<u8> {
    let mut header = vec![1, 0, 0, 0];
    header.extend([0; 16]);
    header.extend(scale.to_be_bytes());
    header.extend(duration.to_be_bytes());
    header.extend([0; 80]);
    boxed(*b"mvhd", &header, false)
}

/// A version 0 track header, of a track shown at `width` by `height`.
fn track_header(width: u16, height: u16) -> Vec<u8> {
    let mut header = vec![0; 4 + 20 + 52];
    header.extend(width.to_be_bytes());
    header.extend([0, 0]);
    header.extend(height.to_be_bytes());
    header.extend([0, 0]);
    boxed(*b"tkhd", &header, false)
}

#[test]
fn a_file_says_how_big_its_picture_is_and_how_long_it_is_wherever_its_moov_is() {
    let sound_first = read(SOUND_FIRST);
    let moov_first = read(MOOV_FIRST);

    assert_eq!(sound_first, About { size: Some((64, 36)), duration: Some(Duration::from_millis(1500)) });
    assert_eq!(moov_first, About { size: Some((48, 32)), duration: Some(Duration::from_secs(2)) });
}

/// A file read through a hub is read a part at a time, so what comes after the `moov` box, such as the picture's
/// frames, isn't read at all.
#[test]
fn nothing_after_the_moov_box_is_read() {
    /// A file that notes the furthest it's been read to.
    struct Noting<'a>(Cursor<&'a [u8]>, u64);
    impl std::io::Read for Noting<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let read = self.0.read(buf)?;
            self.1 = self.1.max(self.0.position());
            Ok(read)
        }
    }
    impl std::io::Seek for Noting<'_> {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.0.seek(to)
        }
    }
    let moov_end = 20 + u64::from(u32::from_be_bytes(MOOV_FIRST[20..24].try_into().unwrap()));
    assert_eq!(&MOOV_FIRST[24..28], b"moov", "the fixture's moov is its second box");
    let mut file = Noting(Cursor::new(MOOV_FIRST), 0);

    about(&mut file).unwrap();

    assert!(file.1 <= moov_end, "read to {}, past the moov box's end at {moov_end}", file.1);
}

/// A box's size can be in eight bytes, or say it goes to the end, and version 1 of a header has its times in eight
/// bytes.
#[test]
fn long_sizes_and_a_box_to_the_end_are_read() {
    let track = boxed(*b"trak", &track_header(320, 240), true);
    let inside = [movie_header(1000, 2500), track].concat();
    let mut file = boxed(*b"ftyp", b"isom", false);
    file.extend(0_u32.to_be_bytes());
    file.extend(b"moov");
    file.extend(inside);

    assert_eq!(read(&file), About { size: Some((320, 240)), duration: Some(Duration::from_millis(2500)) });
}

/// What a file can't say, it doesn't, and a box that says it's bigger than what holds it ends the reading there.
#[test]
fn what_a_file_doesn_t_say_is_none() {
    let unknown = [movie_header(600, u64::MAX), movie_header(0, 10)].concat();
    let past_the_end = [boxed(*b"free", &[], false), 1000_u32.to_be_bytes().to_vec(), b"moov".to_vec()].concat();

    assert_eq!(read(&boxed(*b"moov", &unknown, false)), About::default(), "a duration not known, and no scale");
    assert_eq!(read(&past_the_end), About::default());
    assert_eq!(read(b""), About::default());
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Whatever a file holds, reading it ends, and doesn't panic. One with bytes changed or cut short is most like a
    /// real one.
    #[test]
    fn any_bytes_are_read_without_panicking(
        at in 0..SOUND_FIRST.len(),
        byte in any::<u8>(),
        cut in 0..SOUND_FIRST.len(),
        noise in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let mut changed = SOUND_FIRST.to_vec();
        changed[at] = byte;
        let _ = about(&mut Cursor::new(&changed));
        let _ = about(&mut Cursor::new(&SOUND_FIRST[..cut]));
        let _ = about(&mut Cursor::new(&noise));
    }
}
