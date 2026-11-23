//! Whether a packet starts a keyframe, which decodes without the frames before it, as its codec's own header says, for
//! a WebM whose blocks don't say.

use crate::libvpx::Codec;

/// Whether `packet`, of a stream in `codec`, is a keyframe: VP8's frame tag says so in its lowest bit, and VP9's
/// uncompressed header in its frame type, after its marker, its profile, and whether it only shows an earlier frame.
pub(crate) fn starts_keyframe(codec: Codec, packet: &[u8]) -> bool {
    let Some(&first) = packet.first() else { return false };
    match codec {
        Codec::Vp8 => first & 1 == 0,
        Codec::Vp9 => {
            let bit = |index: u32| first >> (7 - index) & 1 == 1;
            let marked = bit(0) && !bit(1);
            let profile = u8::from(bit(3)) << 1 | u8::from(bit(2));
            // Profile 3 has a reserved bit before the rest.
            let next = if profile == 3 { 5 } else { 4 };
            let (shows_an_earlier_frame, inter) = (bit(next), bit(next + 1));
            marked && !shows_an_earlier_frame && !inter
        }
    }
}

#[cfg(test)]
mod tests {
    use super::starts_keyframe;
    use crate::libvpx::Codec;
    use matroska_demuxer::{Frame, MatroskaFile};
    use std::io::Cursor;

    /// What a packet's own header says agrees with what its block says, in every packet of a file in each codec.
    #[test]
    fn a_packet_s_header_says_what_its_block_says_of_it() {
        let files = [
            (Codec::Vp8, include_bytes!("../tests/decoding/fixtures/vp8-keyframes.webm").as_slice()),
            (Codec::Vp9, include_bytes!("../tests/decoding/fixtures/vp9-odd-size.webm").as_slice()),
        ];
        for (codec, bytes) in files {
            let mut file = MatroskaFile::open(Cursor::new(bytes)).unwrap();
            let (mut packet, mut keyframes) = (Frame::default(), 0);

            while file.next_frame(&mut packet).unwrap() {
                let said = packet.is_keyframe.unwrap();
                assert_eq!(starts_keyframe(codec, &packet.data), said, "{codec:?} at {}", packet.timestamp);
                keyframes += usize::from(said);
            }
            assert!(keyframes > 0 && keyframes < 20, "{codec:?} has {keyframes}");
        }
        assert!(!starts_keyframe(Codec::Vp9, &[]), "nothing is no keyframe");
    }
}
