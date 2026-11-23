//! A file read a part at a time, as a player reads one from a hub, asking for each part as it needs it. A WebM's reader
//! goes back and forth, to the file's index at its end and back, so the parts read last are kept. One whose start
//! doesn't say where its tracks and index are, in a `SeekHead`, is read through as it's opened, part after part, since
//! the reader finds them by passing over every cluster.

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::num::NonZeroU32;

/// Part of a file, as whatever fetches it gives it.
pub struct Part {
    /// As many bytes as were asked for, or fewer where the file ends.
    pub data: Vec<u8>,
    /// How many bytes the whole file had as it was read.
    pub file_bytes: u64,
}

/// What reads part of a file, such as from a hub: `length` bytes of it from `start`, or fewer where it ends. A closure
/// that does is one.
pub trait Fetch {
    fn fetch(&mut self, start: u64, length: u32) -> io::Result<Part>;
}

impl<F: FnMut(u64, u32) -> io::Result<Part>> Fetch for F {
    fn fetch(&mut self, start: u64, length: u32) -> io::Result<Part> {
        self(start, length)
    }
}

/// A file read through `fetch`, which gives `length` bytes of it from `start`, as [`Part`]s of `part_bytes`, keeping
/// the last `kept` it read.
pub struct Parts<F> {
    fetch: F,
    part_bytes: u64,
    file_bytes: u64,
    position: u64,
    kept: usize,
    /// By the part's index, the one read last first.
    read: VecDeque<(u64, Vec<u8>)>,
}

impl<F: Fetch> Parts<F> {
    /// Reads the file's first part, which says how long it is.
    pub fn new(mut fetch: F, part_bytes: NonZeroU32, kept: usize) -> io::Result<Self> {
        let first = fetch.fetch(0, part_bytes.get())?;
        let file_bytes = first.file_bytes;
        let read = VecDeque::from([(0, first.data)]);
        Ok(Self { fetch, part_bytes: u64::from(part_bytes.get()), file_bytes, position: 0, kept: kept.max(1), read })
    }

    pub fn file_bytes(&self) -> u64 {
        self.file_bytes
    }

    /// Part `index`, kept or read now, put first among those kept.
    fn part(&mut self, index: u64) -> io::Result<&[u8]> {
        if let Some(at) = self.read.iter().position(|(kept, _)| *kept == index) {
            let part = self.read.remove(at).unwrap_or_default();
            self.read.push_front(part);
        } else {
            let length = u32::try_from(self.part_bytes).unwrap_or(u32::MAX);
            let Part { data, file_bytes } = self.fetch.fetch(index * self.part_bytes, length)?;
            if file_bytes != self.file_bytes {
                return Err(io::Error::other("the file changed while it was read"));
            }
            self.read.truncate(self.kept - 1);
            self.read.push_front((index, data));
        }
        Ok(self.read.front().map_or(&[], |(_, data)| data.as_slice()))
    }
}

impl<F: Fetch> Read for Parts<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.file_bytes || buf.is_empty() {
            return Ok(0);
        }
        let (index, offset) = (self.position / self.part_bytes, self.position % self.part_bytes);
        let part = self.part(index)?;
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        let Some(available) = part.get(offset..).filter(|available| !available.is_empty()) else {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "a part of the file came back short"));
        };
        let length = available.len().min(buf.len());
        buf[..length].copy_from_slice(&available[..length]);
        self.position += u64::try_from(length).unwrap_or(u64::MAX);
        Ok(length)
    }
}

impl<F> Seek for Parts<F> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let position = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::End(by) => self.file_bytes.checked_add_signed(by),
            SeekFrom::Current(by) => self.position.checked_add_signed(by),
        };
        self.position =
            position.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a seek before the start"))?;
        Ok(self.position)
    }
}
