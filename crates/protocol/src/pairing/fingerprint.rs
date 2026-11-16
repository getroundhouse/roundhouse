//! What a device pins when it pairs with a hub.

use std::fmt;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

/// A SHA-256 hash in unpadded base64url.
const CHARS: usize = 43;

/// The SHA-256 of a hub certificate's `SubjectPublicKeyInfo`, in unpadded base64url. It's the key that's pinned
/// rather than the whole certificate, so the same key could be certified again. The hub works it out from its
/// certificate and a device from what a hub presents, and both hash the same bytes, so the two agree. Plain text on
/// the wire.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Fingerprint(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a fingerprint is {CHARS} characters of unpadded base64url")]
pub struct InvalidFingerprint;

impl Fingerprint {
    /// The fingerprint with this SHA-256 of a `SubjectPublicKeyInfo`.
    pub fn of_sha256(digest: [u8; 32]) -> Fingerprint {
        Fingerprint(URL_SAFE_NO_PAD.encode(digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Fingerprint {
    type Error = InvalidFingerprint;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let base64url = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
        if text.len() == CHARS && text.bytes().all(base64url) { Ok(Fingerprint(text)) } else { Err(InvalidFingerprint) }
    }
}

impl FromStr for Fingerprint {
    type Err = InvalidFingerprint;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Fingerprint::try_from(text.to_string())
    }
}

impl From<Fingerprint> for String {
    fn from(fingerprint: Fingerprint) -> String {
        fingerprint.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_its_hash_in_unpadded_base64url_and_nothing_else_reads_as_one() {
        let fingerprint = Fingerprint::of_sha256([0xfb; 32]);

        assert_eq!(fingerprint.as_str(), format!("{}-_s", "-_v7".repeat(10)));
        assert_eq!(fingerprint.as_str().parse(), Ok(fingerprint.clone()));
        assert_eq!(serde_json::to_string(&fingerprint).unwrap(), format!("\"{fingerprint}\""));
        for not_one in ["", "abc", &"a".repeat(44), &format!("{}=", "a".repeat(42)), &"+".repeat(43)] {
            assert_eq!(not_one.parse::<Fingerprint>(), Err(InvalidFingerprint), "{not_one}");
        }
        assert!(serde_json::from_str::<Fingerprint>("\"abc\"").is_err());
    }
}
