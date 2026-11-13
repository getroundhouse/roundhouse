//! Every corner the app rounds, named by what it's on rather than by its size, so a corner means the same wherever it's
//! drawn, and two that are the same size today can change apart, and the widths of an edge and of the lines that
//! stand for one. `docs/spec/look.md` has the table of the corners, which a test holds to these.
//!
//! They're in pixels rather than rems, as a card's and a control's were in the kit's theme, so a corner stays as round
//! whatever size text is set at.

use gpui_kit::{Pixels, Styled, px};

use crate::named::named;

/// The width of every edge: around a card, along a header, between rows, and around something to use. Most are drawn
/// with the kit's `border_1`, which is this width.
pub const EDGE: Pixels = px(1.);
/// The width of the rule at the left of a quote, such as what a subagent was asked, which stands for an edge on that
/// side alone, so it's wider than one.
pub const QUOTE_RULE: Pixels = px(2.);
/// The width of the line under the tab that's showing.
pub const TAB_LINE: Pixels = px(2.);

named! {
    /// A corner, by what it's on. [`Shaped::corners`] rounds an element's as one.
    Corner {
        /// Something to use, such as a button, a field, a switch, a row to choose in a list, as Settings' pages and
        /// the outline's are, or the kit's menus and tooltips, and what's set small beside them, in a chat's column or
        /// inside a card, such as a pill, a chip, a prompt's bubble, a prompt waiting in the queue, an attachment, a
        /// question's option or what a question's tool was given.
        Control,
        /// A card, and what the app draws over the rest, such as the outline, the find bar or what a hover shows, a
        /// dialog, a composer, and the question panel in its place, and what's set across a chat's column, such as a
        /// card in the chat, a call's block, the compaction's summary or the queue.
        Card,
        /// A control about as tall as a line of text, such as Settings' key or a question's checkbox.
        InlineControl,
        /// What's drawn a few pixels across, or as a fine line around something, such as the square in Stop, the
        /// image diff's swatch, or its outline around what changed between two images.
        Glyph,
    }
}

impl Corner {
    /// How round it is.
    pub const fn radius(self) -> Pixels {
        match self {
            Corner::Control => px(6.),
            Corner::Card => px(10.),
            Corner::InlineControl => px(4.),
            Corner::Glyph => px(2.),
        }
    }

    /// How the spec's table writes it: its name and how round it is.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}px", f32::from(self.radius()))]
    }
}

/// Rounds an element's corners as one of the corners the app has.
pub trait Shaped: Styled + Sized {
    /// Rounds all four of this element's corners as `corner`.
    #[must_use]
    fn corners(self, corner: Corner) -> Self {
        self.rounded(corner.radius())
    }

    /// Rounds only this element's top corners as `corner`, for something that sits on what's below it, as the queue
    /// does on the composer, so the two read as one.
    #[must_use]
    fn corners_top(self, corner: Corner) -> Self {
        self.rounded_t(corner.radius())
    }
}

impl<E: Styled> Shaped for E {}

#[cfg(test)]
mod tests;
