//! The app icon: the fan in copper on a warm dark tile, with a drawing of its own for each size macOS keeps one for.
//! It comes in two styles, and the gallery draws each into its own iconset in `apps/desktop/icons/`, and checks it
//! there.

mod pixels;

mod drawing;
#[cfg(test)]
use drawing::Drawing;
pub use drawing::{IconSize, IconStyle, icon};
