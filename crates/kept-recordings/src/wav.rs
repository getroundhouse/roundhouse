//! The audio's file: a WAV of 32-bit floats in one channel, which is how the model takes it.

/// How long the header is, which the samples follow.
pub const HEADER_BYTES: usize = 44;

/// The most data a WAV file can say it has, which is what one says while it's still being written, so what's there can
/// be read however the writing ended.
pub const MAX_DATA_BYTES: u32 = u32::MAX - 36;

/// A WAV's format for samples that are floats.
const IEEE_FLOAT: u16 = 3;
const BYTES_A_SAMPLE: u16 = 4;

/// The header of a file of 32-bit floats at `sample_rate` in one channel, which take `data_bytes`.
pub fn header(sample_rate: u32, data_bytes: u32) -> [u8; HEADER_BYTES] {
    let data_bytes = data_bytes.min(MAX_DATA_BYTES);
    let mut header = [0; HEADER_BYTES];
    let mut at = 0;
    let mut put = |bytes: &[u8]| {
        header[at..at + bytes.len()].copy_from_slice(bytes);
        at += bytes.len();
    };
    put(b"RIFF");
    put(&(data_bytes + 36).to_le_bytes());
    put(b"WAVEfmt ");
    put(&16_u32.to_le_bytes());
    put(&IEEE_FLOAT.to_le_bytes());
    put(&1_u16.to_le_bytes());
    put(&sample_rate.to_le_bytes());
    put(&(sample_rate * u32::from(BYTES_A_SAMPLE)).to_le_bytes());
    put(&BYTES_A_SAMPLE.to_le_bytes());
    put(&(BYTES_A_SAMPLE * 8).to_le_bytes());
    put(b"data");
    put(&data_bytes.to_le_bytes());
    header
}

#[cfg(test)]
mod tests {
    use super::{MAX_DATA_BYTES, header};

    #[test]
    fn it_says_floats_at_the_rate_in_one_channel() {
        let header = header(16_000, 8);
        assert_eq!((&header[..4], &header[8..16], &header[36..40]), (&b"RIFF"[..], &b"WAVEfmt "[..], &b"data"[..]));
        let short = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
        let long = |at: usize| u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
        assert_eq!((short(20), short(22), short(32), short(34)), (3, 1, 4, 32));
        assert_eq!((long(4), long(24), long(28), long(40)), (44, 16_000, 64_000, 8));
    }

    #[test]
    fn one_still_being_written_says_the_most_it_can() {
        let header = header(16_000, MAX_DATA_BYTES);
        assert_eq!(&header[4..8], u32::MAX.to_le_bytes());
    }
}
