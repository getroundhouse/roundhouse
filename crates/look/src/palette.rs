//! The colours the kit's theme has no name for. Everything else comes from the theme.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Hsla, rgb};

pub(crate) fn hex(value: u32) -> Hsla {
    Hsla::from(rgb(value))
}

/// The brand colour, copper: empty states, the app icon and keyboard focus. Never a status.
pub fn accent() -> Hsla {
    hex(0x00d0_8a5c)
}

/// Quieter than muted text: line numbers, meta, and a mark at rest.
pub fn faint() -> Hsla {
    hex(0x006f_6964)
}

/// A surface one step up from the pane it's on: messages, cards and the selected tab.
pub fn raised(cx: &App) -> Hsla {
    cx.theme().secondary
}
