//! Where each leaf of a text was laid out in a frame, which turns a point in the window into a place in the leaf: the
//! kit says where it drew a leaf of Markdown, and a leaf of plain text is laid out by the transcript, in `Pieces`.

use gpui_kit::base::text::editor_host::LeafGeometry;
use gpui_kit::{Bounds, Pixels, Point};

use super::Pieces;

/// Where a leaf was laid out in this frame.
pub(crate) enum Geometry {
    /// A paragraph, heading, code block or table cell of Markdown, as the kit drew it.
    Markdown(LeafGeometry),
    /// A plain text, which is one leaf.
    Plain(Pieces),
}

impl Geometry {
    /// Where it is in the window.
    pub(super) fn bounds(&self) -> Bounds<Pixels> {
        match self {
            Geometry::Markdown(leaf) => leaf.bounds(),
            Geometry::Plain(pieces) => pieces.bounds(),
        }
    }

    /// Whether any of it shows in `bounds`. A leaf of Markdown is all text, and plain text can have space between its
    /// pieces, so it shows only if a piece does, not just the space between two.
    pub(super) fn shows(&self, bounds: Bounds<Pixels>) -> bool {
        match self {
            Geometry::Markdown(leaf) => leaf.bounds().intersects(&bounds),
            Geometry::Plain(pieces) => pieces.shows(bounds),
        }
    }

    /// How long its text is, in bytes.
    pub(super) fn len(&self) -> usize {
        match self {
            Geometry::Markdown(leaf) => leaf.len(),
            Geometry::Plain(pieces) => pieces.len(),
        }
    }

    /// The place in it nearest `at`, between two characters, on the row across from `at`, or the nearest row, in what
    /// shows of it, `shows`, where an area that scrolls it cuts it short, for a leaf that [`Geometry::shows`] there. A
    /// leaf of Markdown has no space between its rows, so the row across from a point in what shows of it shows too.
    /// Plain text can have space between its pieces, where the row across is in the next piece, which a place is found
    /// in only if it shows.
    pub(super) fn index_for_position(&self, at: Point<Pixels>, shows: Bounds<Pixels>) -> usize {
        match self {
            Geometry::Markdown(leaf) => leaf.index_for_position(at),
            Geometry::Plain(pieces) => pieces.index_for_position(at, shows),
        }
    }
}
