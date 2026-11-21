//! Where each leaf of a text was laid out in a frame, which turns a point in the window into a place in the leaf: the
//! kit says where it drew a leaf of Markdown, and a leaf of plain text is laid out by the transcript, in `Pieces`.

use gpui_kit::base::text::editor_host::LeafGeometry;
use gpui_kit::{Bounds, Pixels, Point, point, px, size};

use super::Pieces;

/// How near two places are to be on the same row, which GPUI can lay out a fraction of a pixel apart.
pub(super) const SAME_ROW: Pixels = px(0.5);

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

    /// Where the place at `byte` is drawn: a line as tall as the row it's on, from the row's top, as a caret there
    /// would be.
    pub(super) fn caret(&self, byte: usize) -> Bounds<Pixels> {
        let (top, row) = match self {
            Geometry::Markdown(leaf) => (leaf.position_for_index(byte), leaf.line_height()),
            Geometry::Plain(pieces) => pieces.caret(byte),
        };
        Bounds::new(top, size(px(1.), row))
    }

    /// Where the place at `byte` is drawn on the row after the one [`Geometry::caret`] has it on, if a row wraps there,
    /// with the character after it, which ends at `next`, on the next row: at that row's start, as a caret there would
    /// be. GPUI draws a place where a row wraps at the end of the row before, and a selection that starts there is
    /// painted from the next, as `Pieces` paints it.
    pub(super) fn caret_on_the_next_row(&self, byte: usize, next: usize) -> Option<Bounds<Pixels>> {
        let (here, after) = (self.caret(byte), self.caret(next));
        let wraps = after.top() >= here.bottom() - SAME_ROW;
        wraps.then(|| Bounds::new(point(self.bounds().left(), after.top()), here.size))
    }

    /// Where its rows are, in blocks with no space between the rows of each: the whole of a leaf of Markdown, and each
    /// piece of a plain text, with the space between two pieces between their blocks.
    pub(super) fn blocks(&self) -> Vec<Bounds<Pixels>> {
        match self {
            Geometry::Markdown(leaf) => vec![leaf.bounds()],
            Geometry::Plain(pieces) => pieces.blocks(),
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
