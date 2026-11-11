//! The colours the kit's theme has no name for. Everything else comes from the theme.

use gpui_kit::{Hsla, rgb};

pub(crate) fn hex(value: u32) -> Hsla {
    Hsla::from(rgb(value))
}

/// Quieter than muted text: line numbers, meta, and a mark with no pull request.
pub fn faint() -> Hsla {
    hex(0x006f_6964)
}

/// The edge of a field on the sidebar, such as Settings' search: lighter than a line, since it's something to type in
/// rather than something that separates.
pub fn field_edge() -> Hsla {
    hex(0x0043_4140)
}

/// The top of the app icon's tile, a step above the pane's colour it fades down to.
pub(crate) fn tile_top() -> Hsla {
    hex(0x0021_1d1b)
}

/// The line under the chat showing's tab: the brand's copper, paled toward the text.
pub fn tab_underline() -> Hsla {
    hex(0x00d8_c2b3)
}

/// A prompt's bubble: a wash of the brand's copper over the pane, so what was asked is easy to find when scrolling past
/// the agent's work.
pub fn prompt() -> Hsla {
    hex(0x0037_281f)
}

/// Behind each match of what's being found in a chat: the pale copper the chat showing is marked with, faint, so the
/// matches read as a trail through the text without hiding it.
pub fn found() -> Hsla {
    hex(0x00d8_c2b3).opacity(0.22)
}

/// Behind the match being shown, in the brand's copper, so it stands out from the rest.
pub fn found_here() -> Hsla {
    hex(0x00d0_8a5c).opacity(0.55)
}
