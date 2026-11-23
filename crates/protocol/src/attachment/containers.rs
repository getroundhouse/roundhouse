//! Which video a file is, from the container its bytes start with: WebM by the doc type its EBML header names, and MP4
//! or QuickTime by the brand its `ftyp` box starts with. An image kept in the same containers, such as HEIF's or
//! AVIF's, isn't a video, and nor is a Matroska file that isn't WebM, which the app doesn't play.

use super::MediaType;

/// What an EBML file starts with, Matroska's and WebM's.
const EBML: [u8; 4] = [0x1a, 0x45, 0xdf, 0xa3];
/// The element in an EBML header that names what kind of file it is.
const DOC_TYPE: u64 = 0x4282;
/// The brands an MP4 starts with, as a video. HEIF's and AVIF's aren't among them.
const MP4_BRANDS: [&[u8; 4]; 10] =
    [b"isom", b"iso2", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"avc1", b"M4V ", b"dash"];

/// The video `bytes` start, if they start one.
pub(super) fn video(bytes: &[u8]) -> Option<MediaType> {
    if bytes.starts_with(&EBML) {
        return (doc_type(&bytes[EBML.len()..])? == b"webm").then_some(MediaType::Webm);
    }
    let brand: &[u8; 4] = bytes.get(8..12)?.try_into().ok()?;
    if bytes.get(4..8)? != b"ftyp" {
        None
    } else if brand == b"qt  " {
        Some(MediaType::Mov)
    } else {
        MP4_BRANDS.contains(&brand).then_some(MediaType::Mp4)
    }
}

/// The doc type the EBML header that `header` starts names, after its ID: its size, then its elements.
fn doc_type(header: &[u8]) -> Option<&[u8]> {
    let (size, length) = number(header, false)?;
    let end = length.saturating_add(usize::try_from(size).ok()?).min(header.len());
    let mut elements = header.get(length..end)?;
    while !elements.is_empty() {
        let (id, id_length) = number(elements, true)?;
        let (size, size_length) = number(elements.get(id_length..)?, false)?;
        let start = id_length + size_length;
        let data = elements.get(start..start.checked_add(usize::try_from(size).ok()?)?)?;
        if id == DOC_TYPE {
            return Some(data);
        }
        elements = &elements[start + data.len()..];
    }
    None
}

/// The number EBML writes at the start of `bytes`, and how many bytes it takes: one to eight, as the place of the first
/// byte's highest bit says, which an ID keeps and a size doesn't.
fn number(bytes: &[u8], keeps_marker: bool) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let zeros = first.leading_zeros();
    if zeros >= 8 {
        return None;
    }
    let marked = if keeps_marker { first } else { first & (0x7f >> zeros) };
    let length = usize::try_from(zeros).ok()? + 1;
    let rest = bytes.get(1..length)?;
    Some((rest.iter().fold(u64::from(marked), |number, byte| number << 8 | u64::from(*byte)), length))
}
