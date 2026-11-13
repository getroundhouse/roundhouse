//! Where the window is, as Windows says, where GPUI's bounds for it are wrong. GPUI reads them from the window's
//! placement, in `WindowsWindowState::calculate_window_bounds` in `gpui-pre-windows`. For a window Windows has
//! arranged, by snapping it to an edge or a corner, or stretching it to its display's height, a placement gives where
//! the window goes back to once it's dragged away. For a minimized one, it says only that it's minimized, whether or
//! not the window goes back to filling the screen. And once Windows has destroyed the window, which can happen while
//! the app quits, before GPUI lets the window go, GPUI makes its bounds up from a placement of nothing.
//!
//! This moves GPUI's bounds by how far the window is from its placement, so it goes in the same GPUI upgrade that has
//! that function read the window's own rect for an ordinary window, as Chromium does. After that upgrade, it would
//! move them twice.
//!
//! What decides and converts is built for the tests everywhere, not only on Windows, since the app's unit tests don't
//! run on Windows.

#[cfg(test)]
use gpui_kit::WindowBounds;
#[cfg(test)]
use gpui_kit::{Bounds, Pixels, point, px, size};

#[cfg(test)]
mod tests;

mod bounds;
#[cfg(target_os = "windows")]
pub(super) use bounds::window_bounds;
#[cfg(not(target_os = "windows"))]
pub(super) use bounds::window_bounds;
#[cfg(test)]
use bounds::{Rect, Shown, bounds, in_placement_terms, moved_to};
