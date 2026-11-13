//! Bytes that travel as base64 text, since a frame is JSON, such as an attachment's or what a terminal's program wrote.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

#[cfg(test)]
mod tests;

/// Declares types of bytes that travel as base64 text, with a type each so that one can't be passed as another. Their
/// debug output is only how many there are, since a frame's would otherwise be a screenful of numbers.
macro_rules! base64_bytes {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name(Vec<u8>);

        impl $name {
            pub fn new(bytes: Vec<u8>) -> Self {
                Self(bytes)
            }

            pub fn as_slice(&self) -> &[u8] {
                &self.0
            }

            pub fn into_vec(self) -> Vec<u8> {
                self.0
            }

            pub fn len(&self) -> usize {
                self.0.len()
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl From<&[u8]> for $name {
            fn from(bytes: &[u8]) -> Self {
                Self(bytes.to_vec())
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "({} bytes)"), self.0.len())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&$crate::base64_bytes::encode(&self.0))
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_str($crate::base64_bytes::Base64Visitor).map(Self)
            }
        }
    )+};
}

pub(crate) use base64_bytes;

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
