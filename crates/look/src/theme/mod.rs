//! The app's theme, over the kit's dark one, made from the app's colours, so the kit's own parts, such as its
//! buttons, menus and rich text, draw with the same ones.

#[cfg(test)]
use crate::colors::Colors;

#[cfg(test)]
mod tests;

mod applying;
pub use applying::apply;
#[cfg(any(test, feature = "testing"))]
pub(crate) use applying::hex;
#[cfg(test)]
use applying::{config, kit_colors};
