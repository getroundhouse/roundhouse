//! Where the window was left: its place and size, the display it was on, and whether it filled the screen, so the app
//! opens as it was last used. It belongs to this computer, as a setting does, but nobody chooses it in Settings, so it
//! has a file of its own in the app's data directory.

#[cfg(test)]
use crate::saving::write_json;
#[cfg(test)]
use gpui_kit::{Bounds, DisplayId, Pixels, WindowBounds, point, px, size};

mod system;
#[cfg(test)]
mod tests;

mod placement;
pub use placement::Opens;
#[cfg(any(test, feature = "testing"))]
pub(crate) use placement::SETTLES_AFTER;
#[cfg(test)]
use placement::{Attached, DisplayUuid, FILE, Mode, Place, read};
pub(crate) use placement::{follow, last_look, open_at};
