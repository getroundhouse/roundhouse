use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::cell::Cell;
use std::io::{self, Cursor, Read as _, Seek as _, SeekFrom};
use std::num::NonZeroU32;
use video::{Part, Parts, WebmVideo};

use super::{THREADS, VP8_KEYFRAMES, hash};

/// What a hub gives of `file`: `length` bytes from `start`, or fewer where it ends.
fn fetching(file: &[u8]) -> impl FnMut(u64, u32) -> io::Result<Part> + '_ {
    move |start, length| {
        let start = usize::try_from(start).unwrap().min(file.len());
        let end = start.saturating_add(usize::try_from(length).unwrap()).min(file.len());
        Ok(Part { data: file[start..end].to_vec(), file_bytes: u64::try_from(file.len()).unwrap() })
    }
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Whatever the part's size and however many are kept, seeking and reading give what the whole file would.
    #[test]
    fn a_file_read_in_parts_reads_as_the_whole_file_would(
        file in prop::collection::vec(any::<u8>(), 0..500),
        part_bytes in 1u32..64,
        kept in 1usize..4,
        moves in prop::collection::vec((0u64..520, 0usize..100), 1..20),
    ) {
        let mut parts = Parts::new(fetching(&file), NonZeroU32::new(part_bytes).unwrap(), kept).unwrap();
        let mut whole = Cursor::new(&file);

        for (to, length) in moves {
            prop_assert_eq!(parts.seek(SeekFrom::Start(to)).unwrap(), whole.seek(SeekFrom::Start(to)).unwrap());
            let (mut read, mut expected) = (vec![0; length], vec![0; length]);
            let (count, expected_count) = (parts.read(&mut read).unwrap(), whole.read(&mut expected).unwrap());
            prop_assert_eq!(&read[..count], &expected[..count]);
            prop_assert!(count > 0 || expected_count == 0, "nothing read where there was more");
            parts.seek(SeekFrom::Start(to + u64::try_from(count).unwrap())).unwrap();
            whole.seek(SeekFrom::Start(to + u64::try_from(count).unwrap())).unwrap();
        }
        prop_assert_eq!(parts.seek(SeekFrom::End(0)).unwrap(), u64::try_from(file.len()).unwrap());
    }
}

/// A video read in parts decodes as it does whole, asking for each part once when the parts kept hold its index too,
/// and a file that changes while it's read fails rather than mixing two files.
#[test]
fn a_video_reads_in_parts_and_a_changed_file_fails() {
    let asked = Cell::new(0);
    let counted = |start, length| {
        asked.set(asked.get() + 1);
        fetching(VP8_KEYFRAMES)(start, length)
    };
    let mut changing = fetching(VP8_KEYFRAMES);
    let grown = move |start, length| {
        let part = changing(start, length)?;
        Ok(Part { file_bytes: part.file_bytes + u64::from(start > 0), ..part })
    };

    let mut video = WebmVideo::open(Parts::new(counted, NonZeroU32::new(1024).unwrap(), 16).unwrap(), THREADS).unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = video.next_frame().unwrap() {
        frames.push(hash(&frame.picture));
    }
    let mut whole = WebmVideo::open(Cursor::new(VP8_KEYFRAMES), THREADS).unwrap();
    let mut expected = Vec::new();
    while let Some(frame) = whole.next_frame().unwrap() {
        expected.push(hash(&frame.picture));
    }
    let changed = Parts::new(grown, NonZeroU32::new(1024).unwrap(), 16).unwrap().read_to_end(&mut Vec::new());

    assert_eq!(frames, expected);
    assert_eq!(asked.get(), VP8_KEYFRAMES.len().div_ceil(1024), "each part once");
    assert_eq!(changed.unwrap_err().kind(), io::ErrorKind::Other);
}
