/// What a WebM starts with, as ffmpeg writes one: its EBML header, with the doc type `webm`, and the ID of the segment
/// that follows, which is all anything but a decoder reads of it to know it's one.
pub const WEBM_START: &[u8] = b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\xf7\x81\x01\x42\xf2\x81\x04\x42\xf3\x81\x08\
    \x42\x82\x84webm\x42\x87\x81\x02\x42\x85\x81\x02\x18\x53\x80\x67";

/// A made-up WebM of `len` bytes, which reads as one by its start and is the same bytes on every run.
pub fn webm(len: usize) -> Vec<u8> {
    let mut bytes = WEBM_START.to_vec();
    bytes.extend((0..len.saturating_sub(WEBM_START.len())).map(|at| u8::try_from(at % 251).unwrap_or(0)));
    bytes.truncate(len);
    bytes
}
