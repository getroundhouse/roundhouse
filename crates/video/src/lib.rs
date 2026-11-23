//! Decodes the WebM videos agents make, VP8 or VP9, a frame at a time, with libvpx, which the build script fetches
//! and compiles, as `third_party/README.md` says, and reads what an MP4 or a QuickTime file says of its video, which
//! the system's player plays. No UI and no async runtime, as `docs/principles/layers.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod bgra;
#[cfg(feature = "testing")]
pub mod fixtures;
mod iso_media;
mod keyframes;
mod libvpx;
mod parts;
mod picture;
mod player;
mod webm;

pub use bgra::Rows;
pub use iso_media::{About, about};
pub use libvpx::{DecodeError, LibvpxError};
pub use parts::{Fetch, Part, Parts};
pub use picture::{Colors, Matrix, Picture, Plane, Subsampling};
pub use player::{Decoded, Player, Shown};
pub use webm::{Frame, OpenError, ReadError, WebmVideo};
