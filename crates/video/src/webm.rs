//! A WebM file's video, read and decoded a frame at a time.

use crate::keyframes::starts_keyframe;
use crate::libvpx::{Codec, DecodeError, Decoder, LibvpxError};
use crate::picture::Picture;
use matroska_demuxer::{DemuxError, Frame as Packet, MatroskaFile};
use std::io::{Read, Seek};
use std::num::NonZeroU32;
use std::time::Duration;

/// The first video in a WebM file, VP8 or VP9, and the decoder that turns it into pictures. Its sound, if any, is
/// passed over.
pub struct WebmVideo<R: Read + Seek> {
    file: MatroskaFile<R>,
    track: u64,
    /// How many nanoseconds a unit of the file's timestamps is.
    timestamp_scale: u64,
    width: u32,
    height: u32,
    codec: Codec,
    decoder: Decoder,
    /// The last packet read, kept so each read reuses its memory.
    packet: Packet,
    /// Whether `packet` is a keyframe a seek found, which the next read decodes before reading another.
    held: bool,
    /// The time, in the file's units, before which a seek's frames are decoded and not shown.
    shown_from: u64,
}

/// Why a file can't be played.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// It isn't WebM, or it's broken before its first frame.
    #[error("couldn't read it as WebM: {0}")]
    Read(#[from] DemuxError),
    #[error("it has no video")]
    NoVideo,
    /// Its video is in a codec other than VP8 or VP9, such as AV1, by its WebM name, such as `V_AV1`.
    #[error("its video is {codec}, not VP8 or VP9")]
    Codec { codec: String },
    #[error(transparent)]
    Libvpx(#[from] LibvpxError),
}

/// Why the next frame couldn't be read.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The file broke off or is broken where the frame should be.
    #[error("couldn't read the next frame: {0}")]
    Read(#[from] DemuxError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// A frame, and when it's shown.
#[derive(Debug)]
pub struct Frame<'a> {
    /// From the start of the video.
    pub time: Duration,
    pub picture: Picture<'a>,
}

impl<R: Read + Seek> WebmVideo<R> {
    /// The first video in the WebM `reader` holds, decoded on up to `threads`, which only VP9 uses.
    pub fn open(reader: R, threads: NonZeroU32) -> Result<Self, OpenError> {
        let file = MatroskaFile::open(reader)?;
        let video = file.tracks().iter().find(|track| track.video().is_some()).ok_or(OpenError::NoVideo)?;
        let codec = match video.codec_id() {
            "V_VP8" => Codec::Vp8,
            "V_VP9" => Codec::Vp9,
            other => return Err(OpenError::Codec { codec: other.to_string() }),
        };
        let (track, size) =
            (video.track_number().get(), video.video().map(|video| (video.pixel_width(), video.pixel_height())));
        let (width, height) = size.map_or((0, 0), |(width, height)| (pixels(width.get()), pixels(height.get())));
        let timestamp_scale = file.info().timestamp_scale().get();
        let decoder = Decoder::new(codec, threads)?;
        let packet = Packet::default();
        Ok(WebmVideo {
            file,
            track,
            timestamp_scale,
            width,
            height,
            codec,
            decoder,
            packet,
            held: false,
            shown_from: 0,
        })
    }

    /// The width and height the file says its frames are, in pixels, which a frame's own can differ from.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// How long the file says it is, when it says.
    pub fn duration(&self) -> Option<Duration> {
        let units = self.file.info().duration()?;
        #[expect(clippy::cast_precision_loss, reason = "a scale of nanoseconds is far below 2^52")]
        let nanoseconds = units * self.timestamp_scale as f64;
        Duration::try_from_secs_f64(nanoseconds / 1e9).ok()
    }

    /// The next frame that's shown, or `None` at the end.
    pub fn next_frame(&mut self) -> Result<Option<Frame<'_>>, ReadError> {
        loop {
            if !std::mem::take(&mut self.held) && !self.next_packet()? {
                return Ok(None);
            }
            // A frame only the next refers to shows nothing, and one before where a seek went isn't shown, so the
            // loop reads on.
            if self.decoder.decode(&self.packet.data)? && self.packet.timestamp >= self.shown_from {
                self.shown_from = 0;
                let time = Duration::from_nanos(self.packet.timestamp.saturating_mul(self.timestamp_scale));
                return Ok(self.decoder.picture().map(|picture| Frame { time, picture }));
            }
        }
    }

    /// Goes to `time`, so the next frame is the first shown at or after it. It's decoded from the last keyframe before
    /// it, which is found by looking further back each time, from a second before, as a file keeps one every few
    /// seconds. A time past the end leaves nothing to read.
    pub fn seek(&mut self, time: Duration) -> Result<(), ReadError> {
        let target = u64::try_from(time.as_nanos() / u128::from(self.timestamp_scale)).unwrap_or(u64::MAX);
        // At least a unit, so a file whose unit is longer than a second still looks further back each time.
        let mut back = (1_000_000_000 / self.timestamp_scale).max(1);
        loop {
            let from = target.saturating_sub(back);
            self.file.seek(from)?;
            let found = self.next_keyframe()?;
            if found.is_some_and(|at| at <= target) || from == 0 {
                self.held = found.is_some();
                self.shown_from = target;
                return Ok(());
            }
            back = back.saturating_mul(4);
        }
    }

    /// Reads on to the next keyframe of the video, and says when it's shown, or `None` if there's none.
    fn next_keyframe(&mut self) -> Result<Option<u64>, ReadError> {
        while self.next_packet()? {
            let keyframe = self.packet.is_keyframe.unwrap_or_else(|| starts_keyframe(self.codec, &self.packet.data));
            if keyframe {
                return Ok(Some(self.packet.timestamp));
            }
        }
        Ok(None)
    }

    /// Reads the next packet of the video into `packet`, passing over any other track's, or says there's none.
    fn next_packet(&mut self) -> Result<bool, ReadError> {
        while self.file.next_frame(&mut self.packet)? {
            if self.packet.track == self.track {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// A size in a WebM file, as a frame's pixels, which libvpx caps at 16384, so anything bigger reads as the most.
fn pixels(size: u64) -> u32 {
    u32::try_from(size).unwrap_or(u32::MAX)
}
