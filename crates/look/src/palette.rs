//! The one colour the kit's theme has no name for. Everything else comes from the theme.

use gpui_kit::{Hsla, rgb};

pub(crate) fn hex(value: u32) -> Hsla {
    Hsla::from(rgb(value))
}

/// Quieter than muted text: line numbers, meta, and a mark with no pull request.
pub fn faint() -> Hsla {
    hex(0x006f_6964)
}
