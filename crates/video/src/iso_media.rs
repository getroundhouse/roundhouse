//! What an MP4 or a QuickTime file says of the video it holds, which the app doesn't play: how big its picture is and
//! how long it is, from the `moov` box, which a file can have at its start or its end. Only the boxes on the way to it
//! are read, by their headers, a few bytes each, so a file read a part at a time is read in a few parts. What a file
//! says is untrusted, so nothing is made as big as a box says it is, and a box that says it's smaller than its header,
//! or goes on past what holds it, ends the reading there.

use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;
use std::time::Duration;

/// The most boxes read at one level, far more than a video has, so a file of many tiny ones is read only so far.
const MOST_BOXES: usize = 4096;

/// What an MP4 or a QuickTime file says of its video.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct About {
    /// The size its first track with a picture is shown at, in pixels.
    pub size: Option<(u32, u32)>,
    pub duration: Option<Duration>,
}

/// What `file`, an MP4 or a QuickTime file, says of its video, as much of it as it says. Fails only if it can't be
/// read.
pub fn about<F: Read + Seek>(file: &mut F) -> io::Result<About> {
    let end = file.seek(SeekFrom::End(0))?;
    let mut about = About::default();
    let Some(movie) = find(file, 0..end, *b"moov")? else { return Ok(about) };
    each_box(file, movie, |file, kind, inside| {
        match &kind {
            b"mvhd" => about.duration = duration(file, inside.start)?,
            b"trak" if about.size.is_none() => {
                if let Some(header) = find(file, inside, *b"tkhd")? {
                    about.size = size(file, header.start)?;
                }
            }
            _ => {}
        }
        Ok(about.duration.is_some() && about.size.is_some())
    })?;
    Ok(about)
}

/// Reads the boxes between `within`'s start and its end in order, giving `each` the kind of each and where what's in it
/// is, until `each` says it has what it looks for.
fn each_box<F: Read + Seek>(
    file: &mut F,
    within: Range<u64>,
    mut each: impl FnMut(&mut F, [u8; 4], Range<u64>) -> io::Result<bool>,
) -> io::Result<()> {
    let mut at = within.start;
    for _ in 0..MOST_BOXES {
        if within.end.saturating_sub(at) < 8 {
            break;
        }
        file.seek(SeekFrom::Start(at))?;
        let [a, b, c, d, kind @ ..] = read::<8>(file)?;
        let (size, header) = match u32::from_be_bytes([a, b, c, d]) {
            // To the end of what holds it.
            0 => (within.end - at, 8),
            // In the eight bytes after its kind.
            1 if within.end - at >= 16 => (u64::from_be_bytes(read::<8>(file)?), 16),
            size => (u64::from(size), 8),
        };
        let Some(end) = at.checked_add(size).filter(|end| size >= header && *end <= within.end) else { break };
        if each(file, kind, at + header..end)? {
            break;
        }
        at = end;
    }
    Ok(())
}

/// Where what's in the first box of `kind` between `within`'s start and its end is, reading no box after it.
fn find<F: Read + Seek>(file: &mut F, within: Range<u64>, kind: [u8; 4]) -> io::Result<Option<Range<u64>>> {
    let mut found = None;
    each_box(file, within, |_, each, inside| {
        if each == kind {
            found = Some(inside);
        }
        Ok(found.is_some())
    })?;
    Ok(found)
}

/// How long the movie whose header starts at `at` says it is, from its time scale and its duration in it, after its
/// times, which version 1 of the header has in eight bytes each, and version 0 in four.
fn duration(file: &mut (impl Read + Seek), at: u64) -> io::Result<Option<Duration>> {
    file.seek(SeekFrom::Start(at))?;
    let [version, ..] = read::<4>(file)?;
    let (scale, duration) = if version == 1 {
        let [.., s0, s1, s2, s3, d0, d1, d2, d3, d4, d5, d6, d7] = read::<28>(file)?;
        (u32::from_be_bytes([s0, s1, s2, s3]), u64::from_be_bytes([d0, d1, d2, d3, d4, d5, d6, d7]))
    } else {
        let [.., s0, s1, s2, s3, d0, d1, d2, d3] = read::<16>(file)?;
        (u32::from_be_bytes([s0, s1, s2, s3]), u64::from(u32::from_be_bytes([d0, d1, d2, d3])))
    };
    // All ones is a duration that isn't known.
    let unknown = if version == 1 { u64::MAX } else { u64::from(u32::MAX) };
    if scale == 0 || duration == unknown {
        return Ok(None);
    }
    let nanos = u128::from(duration) * 1_000_000_000 / u128::from(scale);
    Ok(u64::try_from(nanos).ok().map(Duration::from_nanos))
}

/// The size the track whose header starts at `at` is shown at, or `None` for one with no picture, such as its sound.
/// Each side is a fixed-point number, 16 bits whole and 16 a fraction, at the header's end, after its times and its
/// track's ID, which version 1 has in 32 bytes, and version 0 in 20, and 52 bytes of its layer, its volume and how
/// it's turned.
fn size(file: &mut (impl Read + Seek), at: u64) -> io::Result<Option<(u32, u32)>> {
    file.seek(SeekFrom::Start(at))?;
    let [version, ..] = read::<4>(file)?;
    let times = if version == 1 { 32 } else { 20 };
    file.seek(SeekFrom::Current(times + 52))?;
    let [w0, w1, _, _, h0, h1, _, _] = read::<8>(file)?;
    let (width, height) = (u32::from(u16::from_be_bytes([w0, w1])), u32::from(u16::from_be_bytes([h0, h1])));
    Ok((width > 0 && height > 0).then_some((width, height)))
}

fn read<const N: usize>(file: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
