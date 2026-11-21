//! A clip's frames as a video: an MP4 file of H.264, which plays wherever a video does, in a browser, in the Mac's own
//! player, in Quick Look and on GitHub, at 60 frames a second, as a display draws. The Mac's own encoder writes it,
//! through `AVFoundation`, so there's nothing to install. H.264 keeps colour at half the resolution it keeps
//! brightness, which a window drawn at twice its size in points barely shows, and the clip's frames are kept as they
//! were drawn besides.

#![cfg(test)]

use std::path::Path;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use image::RgbaImage;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus, AVFileTypeMPEG4,
    AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey,
    AVVideoExpectedSourceFrameRateKey, AVVideoHeightKey, AVVideoProfileLevelH264HighAutoLevel, AVVideoProfileLevelKey,
    AVVideoWidthKey,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
    kCVReturnSuccess,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::gallery::steps::FPS;

/// Bits a second the encoder aims for. Text has hard edges, which take more than film does to keep sharp.
const BITS_A_SECOND: i64 = 16_000_000;
/// Longer than the encoder takes to be ready for another frame, or to finish a clip of a few seconds, many times over.
const FINISHING: Duration = Duration::from_secs(60);

/// A video being written, a frame at a time.
pub(crate) struct Video {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    width: u32,
    height: u32,
    /// How many frames it has, which is when the next one shows, in frames.
    frames: i64,
}

impl Video {
    /// Starts one at `path`, where nothing is, since the writer won't write over a file, of frames `width` by
    /// `height`, which are even, since H.264 keeps colour for each square of four pixels.
    pub(crate) fn start(path: &Path, width: u32, height: u32) -> Self {
        assert!(
            width.is_multiple_of(2) && height.is_multiple_of(2),
            "a frame {width} by {height} isn't even both ways"
        );
        assert!(!path.exists(), "{path:?} is there already");
        let writer = writer_at(path);
        let input = video_input(width, height);
        let adaptor = adaptor_for(&input);
        start_writing(&writer, &input);
        Self { writer, input, adaptor, width, height, frames: 0 }
    }

    /// Adds `frame` as the next, a frame after the one before.
    pub(crate) fn add(&mut self, frame: &RgbaImage) {
        assert_eq!(frame.dimensions(), (self.width, self.height), "a clip's frames are all the same size");
        let buffer = pixels_of(frame);
        let (writer, input) = (&self.writer, &self.input);
        waiting::thread::until(FINISHING, "the encoder to be ready for another frame", || {
            ready_for_more(input) || failed(writer)
        });
        append(&self.writer, &self.adaptor, &buffer, self.frames);
        self.frames += 1;
    }

    /// Writes what's left, and closes the file.
    pub(crate) fn finish(self) {
        let (finished, wait) = mpsc::channel();
        finish_writing(&self.writer, &self.input, move || {
            // The writer calls this once, and nothing else waits on it.
            let _ = finished.send(());
        });
        wait.recv_timeout(FINISHING).expect("the encoder to finish the video");
        // SAFETY: a property the writer sets as it finishes, read once it has.
        let status = unsafe { self.writer.status() };
        // SAFETY: as for the status.
        let error = unsafe { self.writer.error() };
        assert_eq!(status, AVAssetWriterStatus::Completed, "the video wasn't written: {error:?}");
    }
}

fn writer_at(path: &Path) -> Retained<AVAssetWriter> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    // SAFETY: AVFoundation's constant, which it sets once as it loads and never changes.
    let mp4 = unsafe { AVFileTypeMPEG4 }.expect("AVFoundation names MP4");
    // SAFETY: a file URL, as the writer needs, and a file type AVFoundation names.
    unsafe { AVAssetWriter::assetWriterWithURL_fileType_error(&url, mp4) }
        .unwrap_or_else(|error| panic!("can't write a video at {path:?}: {error:?}"))
}

/// What takes the frames, which it encodes as H.264 `width` by `height`, at [`BITS_A_SECOND`].
fn video_input(width: u32, height: u32) -> Retained<AVAssetWriterInput> {
    // SAFETY: AVFoundation's constants, which it sets once as it loads and never changes.
    let keys = unsafe {
        [
            AVVideoCodecKey,
            AVVideoWidthKey,
            AVVideoHeightKey,
            AVVideoCompressionPropertiesKey,
            AVVideoAverageBitRateKey,
            AVVideoProfileLevelKey,
            AVVideoExpectedSourceFrameRateKey,
        ]
    };
    let [codec, width_key, height_key, compression_key, bits_key, profile_key, rate_key] =
        keys.map(|key| key.expect("AVFoundation names its video settings"));
    // SAFETY: as for the keys.
    let (h264, high) = unsafe { (AVVideoCodecTypeH264, AVVideoProfileLevelH264HighAutoLevel) };
    let (h264, high) = (h264.expect("AVFoundation names H.264"), high.expect("AVFoundation names H.264's profiles"));
    let (bits, rate) = (NSNumber::new_i64(BITS_A_SECOND), NSNumber::new_u32(FPS));
    let compression: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[bits_key, profile_key, rate_key], &[bits.as_ref(), high.as_ref(), rate.as_ref()]);
    let (width, height) = (NSNumber::new_u32(width), NSNumber::new_u32(height));
    let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
        &[codec, width_key, height_key, compression_key],
        &[h264.as_ref(), width.as_ref(), height.as_ref(), compression.as_ref()],
    );
    // SAFETY: AVFoundation's constant, as for the keys.
    let video = unsafe { AVMediaTypeVideo }.expect("AVFoundation names video");
    // SAFETY: settings for compressed video, with the codec, the width and the height it needs, each of the type its
    // key takes.
    let input = unsafe { AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(video, Some(&settings)) };
    // SAFETY: set before the input is added to a writer, as it has to be.
    unsafe { input.setExpectsMediaDataInRealTime(false) };
    input
}

/// What hands `input` the frames as pixel buffers, which come in their own format, so it has no pool of its own.
fn adaptor_for(input: &AVAssetWriterInput) -> Retained<AVAssetWriterInputPixelBufferAdaptor> {
    // SAFETY: an input for video, not yet added to a writer or to another adaptor.
    unsafe {
        AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
            input, None,
        )
    }
}

fn start_writing(writer: &AVAssetWriter, input: &AVAssetWriterInput) {
    // SAFETY: the writer hasn't started, so it can take an input, and the session starts once it has.
    unsafe {
        assert!(writer.canAddInput(input), "the writer can't take video");
        writer.addInput(input);
        assert!(writer.startWriting(), "the writer didn't start: {:?}", writer.error());
        writer.startSessionAtSourceTime(CMTime::new(0, timescale()));
    }
}

/// Whether the writer has stopped on an error, which appending the next frame says.
fn failed(writer: &AVAssetWriter) -> bool {
    // SAFETY: a property, read from the thread that appends.
    unsafe { writer.status() == AVAssetWriterStatus::Failed }
}

fn ready_for_more(input: &AVAssetWriterInput) -> bool {
    // SAFETY: a property, read from the thread that appends.
    unsafe { input.isReadyForMoreMediaData() }
}

/// `frame`, in a pixel buffer of its own, as BGRA, which the encoder takes.
fn pixels_of(frame: &RgbaImage) -> CFRetained<CVPixelBuffer> {
    let (width, height) = frame.dimensions();
    let mut made: *mut CVPixelBuffer = std::ptr::null_mut();
    let (columns, rows) = (usize::try_from(width).unwrap(), usize::try_from(height).unwrap());
    // SAFETY: `made` is where the new buffer goes, and the format is one Core Video makes buffers in.
    let status =
        unsafe { CVPixelBufferCreate(None, columns, rows, kCVPixelFormatType_32BGRA, None, NonNull::from(&mut made)) };
    assert_eq!(status, kCVReturnSuccess, "Core Video didn't make a pixel buffer");
    // SAFETY: Core Video made it, and it's ours to release.
    let buffer = unsafe { CFRetained::from_raw(NonNull::new(made).expect("Core Video made a pixel buffer")) };
    // SAFETY: locked to write, and unlocked below, as the base address needs.
    let locked = unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) };
    assert_eq!(locked, kCVReturnSuccess, "Core Video didn't lock the pixel buffer");
    let (base, stride) = (CVPixelBufferGetBaseAddress(&buffer).cast::<u8>(), CVPixelBufferGetBytesPerRow(&buffer));
    assert!(!base.is_null() && stride >= columns * 4, "the pixel buffer holds a row of the frame");
    for (y, row) in frame.rows().enumerate() {
        // SAFETY: the buffer is locked, and holds `rows` rows of `stride` bytes, of which a row of the frame is the
        // first `columns * 4`.
        let into = unsafe { std::slice::from_raw_parts_mut(base.add(y * stride), columns * 4) };
        for (pixel, out) in row.zip(into.as_chunks_mut::<4>().0) {
            let [r, g, b, a] = pixel.0;
            *out = [b, g, r, a];
        }
    }
    // SAFETY: locked above, with the same flags.
    let unlocked = unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) };
    assert_eq!(unlocked, kCVReturnSuccess, "Core Video didn't unlock the pixel buffer");
    buffer
}

/// Appends `buffer` to show at frame `at`.
fn append(writer: &AVAssetWriter, adaptor: &AVAssetWriterInputPixelBufferAdaptor, buffer: &CVPixelBuffer, at: i64) {
    // SAFETY: a frame's time, in frames at FPS, which is a valid time.
    let at = unsafe { CMTime::new(at, timescale()) };
    // SAFETY: the session has started, the input is ready for more, and the buffer is unlocked.
    let appended = unsafe { adaptor.appendPixelBuffer_withPresentationTime(buffer, at) };
    // SAFETY: as for the status once it's written.
    assert!(appended, "the writer didn't take a frame: {:?}", unsafe { writer.error() });
}

/// Marks `input` finished, and has `writer` call `done` once the file's written.
fn finish_writing(writer: &AVAssetWriter, input: &AVAssetWriterInput, done: impl Fn() + Send + 'static) {
    let done = block2::RcBlock::new(done);
    // SAFETY: every frame has been appended, and the block only sends on a channel, which is `Send`.
    unsafe {
        input.markAsFinished();
        writer.finishWritingWithCompletionHandler(&done);
    }
}

/// A frame's length, as a video's times count them.
fn timescale() -> i32 {
    i32::try_from(FPS).unwrap()
}
