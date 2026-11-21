//! The feed's public key, and checking the feed's signature with it.

use ring::signature::{ED25519, UnparsedPublicKey};

/// The update feed's public key, the raw 32 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateFeedKey(pub [u8; 32]);

/// A feed whose signature doesn't match it.
#[derive(Debug, thiserror::Error)]
#[error("the update feed's signature doesn't match it, so it may not come from Roundhouse's release workflow")]
pub struct Unsigned;

impl UpdateFeedKey {
    /// Decodes the key from base64 while compiling, so a build given a malformed one fails, rather than the program.
    pub const fn from_base64(text: &str) -> Self {
        let text = text.as_bytes();
        assert!(text.len() == 44 && text[43] == b'=' && text[42] != b'=', "a feed key is 32 bytes in base64");
        // Each character is 6 bits. `held` keeps the `bits` that don't make a whole byte yet.
        let (mut key, mut written, mut held, mut bits, mut i) = ([0u8; 32], 0, 0u32, 0u32, 0);
        while i < 43 {
            held = (held << 6) | sextet(text[i]);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                key[written] = low_byte(held >> bits);
                written += 1;
                held &= (1 << bits) - 1;
            }
            i += 1;
        }
        assert!(held == 0, "a feed key's base64 has to end in a 0, 4, 8, A, E, I, M, Q, U, Y, c, g, k, o, s or w");
        UpdateFeedKey(key)
    }

    /// Whether `signature` is this key's of exactly `feed`. Nothing should read the feed before it is.
    pub fn verify(&self, feed: &[u8], signature: &[u8]) -> Result<(), Unsigned> {
        UnparsedPublicKey::new(&ED25519, self.0).verify(feed, signature).map_err(|_| Unsigned)
    }
}

const fn sextet(c: u8) -> u32 {
    let value = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("a feed key is base64"),
    };
    value as u32
}

#[expect(clippy::cast_possible_truncation, reason = "the caller has already shifted away all but the low byte")]
const fn low_byte(value: u32) -> u8 {
    value as u8
}
