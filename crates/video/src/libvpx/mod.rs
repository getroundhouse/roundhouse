//! libvpx's VP8 and VP9 decoders, which the build script fetches and compiles, as `third_party/README.md` says.

mod decoder;
mod ffi;

pub(crate) use decoder::{Codec, Decoder};
pub use decoder::{DecodeError, LibvpxError};
