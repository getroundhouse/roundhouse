//! The app's theme, over the kit's dark one, made from the app's colours, so the kit's own parts, such as its
//! buttons, menus and rich text, draw with the same ones.

#[cfg(test)]
use crate::colors::Colors;

#[cfg(test)]
mod tests;

mod applying;
#[cfg(test)]
use applying::config;
#[cfg(any(test, feature = "testing"))]
#[cfg(test)]
pub(crate) use applying::kit_colors;
pub(crate) use applying::{CHOSEN, PRESSED, UNDER_POINTER};
pub use applying::{apply, hex};
