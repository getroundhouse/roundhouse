//! Bytes to base64 text and back.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

pub(crate) fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub(crate) fn decode(text: &str) -> Result<Vec<u8>, base64::DecodeError> {
    STANDARD.decode(text)
}

/// Decodes the text where it lies, rather than copying it into a `String` first, which for an image is megabytes.
pub(crate) struct Base64Visitor;

impl serde::de::Visitor<'_> for Base64Visitor {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("base64 text")
    }

    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Vec<u8>, E> {
        decode(text).map_err(E::custom)
    }
}
