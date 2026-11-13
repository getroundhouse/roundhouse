//! A plain text as the transcript lays it out: in pieces, each a text of its own, such as a line of what a command
//! printed, in order and a line break apart, which together say all of it. They turn a point into a place in the text,
//! for a press, and paint what's selected of it: behind the pieces, to the edge of a row that goes on into the next,
//! and over the space between two pieces a selection runs across, so it reads as one. The kit does both for a leaf of
//! Markdown, and makes neither for a text it didn't lay out, so this follows what it does.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{Bounds, Pixels, Point, TextLayout, point, px};

/// A plain text's pieces, each with the byte of the text it starts at. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Pieces(Rc<[Piece]>);

/// One of a text's pieces: where in the text it starts, and how it's laid out.
struct Piece {
    start: usize,
    layout: TextLayout,
}

impl Piece {
    /// Where in the text it ends, before the line break after it.
    fn end(&self) -> usize {
        self.start + self.layout.len()
    }

    /// Whether it shows in `bounds`.
    fn shows(&self, bounds: Bounds<Pixels>) -> bool {
        self.layout.bounds().intersects(&bounds)
    }
}

impl Pieces {
    /// A text laid out whole, in `layout`.
    pub(crate) fn whole(layout: TextLayout) -> Self {
        Self(Rc::from([Piece { start: 0, layout }]))
    }

    /// A text laid out a piece at a time: each of `texts`, in order, a line break apart, laid out by `lay_out`, which
    /// is given where the piece is in the whole text, and the piece's text.
    pub(crate) fn lines<'a>(
        texts: impl IntoIterator<Item = &'a str>,
        mut lay_out: impl FnMut(Range<usize>, &'a str) -> TextLayout,
    ) -> Self {
        let mut start = 0;
        let pieces = texts.into_iter().map(|text| {
            let piece = Piece { start, layout: lay_out(start..start + text.len(), text) };
            start += text.len() + 1;
            piece
        });
        Self(pieces.collect())
    }

    /// Whether they lay out `said`, each piece what it says from where it starts, with a line break between one and
    /// the next, so a place in it is where it's painted.
    pub(crate) fn lay_out(&self, said: &str) -> bool {
        let mut next = 0;
        let each = self.0.iter().all(|piece| {
            let text = piece.layout.text();
            let fits = piece.start == next && said.get(piece.start..piece.end()) == Some(text.as_str());
            next = piece.end() + 1;
            // The last piece ends where the text does, and every other with the line break before the next.
            fits && said.get(piece.end()..next).is_none_or(|after| after == "\n")
        });
        each && next == said.len() + 1
    }

    /// Where they are in the window, all together.
    pub(super) fn bounds(&self) -> Bounds<Pixels> {
        let each = self.0.iter().map(|piece| piece.layout.bounds());
        each.reduce(|all, piece| all.union(&piece)).unwrap_or_default()
    }

    /// How long the text they say is, in bytes.
    pub(super) fn len(&self) -> usize {
        self.0.last().map_or(0, Piece::end)
    }

    /// Whether any piece shows in `bounds`, rather than only the space between two.
    pub(super) fn shows(&self, bounds: Bounds<Pixels>) -> bool {
        self.0.iter().any(|piece| piece.shows(bounds))
    }

    /// The place in the text nearest `at`, in a piece that shows in `shows`: on the row across from it, or the nearest
    /// row. A point between two pieces is across from the first row of the one below it, if that shows, and otherwise
    /// is on the last row of the one above, so where an area cuts the text short in the space between two pieces, no
    /// point finds a place in the one it's moved out of sight. With none showing, it's the text's start.
    pub(super) fn index_for_position(&self, at: Point<Pixels>, shows: Bounds<Pixels>) -> usize {
        let showing: Vec<&Piece> = self.0.iter().filter(|piece| piece.shows(shows)).collect();
        let piece = showing.iter().find(|piece| at.y < piece.layout.bounds().bottom()).or(showing.last());
        piece.map_or(0, |piece| piece.start + nearest(&piece.layout, at))
    }

    /// Where `range` of what they say, `said`, is painted. A selection that starts where a line wraps is painted from
    /// the start of the row it wraps onto, since GPUI puts that place at the end of the row before.
    pub(crate) fn selected_boxes(&self, range: Range<usize>, said: &str) -> Vec<Bounds<Pixels>> {
        let spans: Vec<Span> = (self.0.iter())
            .filter(|piece| range.start <= piece.end() && piece.start <= range.end)
            .filter_map(|piece| {
                let within = range.start.max(piece.start) - piece.start..range.end.min(piece.end()) - piece.start;
                Some(span(&piece.layout, said.get(piece.start..piece.end())?, within))
            })
            .collect();
        boxes(&spans)
    }
}

/// The place in `layout` nearest `at`, as the kit finds it in a leaf of Markdown, with its private `whole_index_at`,
/// which this follows, so a kit upgrade that changes one should change both. A character's halves go to its nearer
/// side, but for a line's last character, which a press anywhere on goes past, as GPUI's `closest_index_for_x` has it.
/// GPUI's own `TextLayout::index_for_position` gives the start of the character under a point, so a press on a
/// character's right half would select from before it in plain text and after it in Markdown.
fn nearest(layout: &TextLayout, at: Point<Pixels>) -> usize {
    let (bounds, row) = (layout.bounds(), layout.line_height());
    let lines = layout.line_layouts();
    let (mut top, mut start) = (bounds.top(), 0);
    for (ix, line) in lines.iter().enumerate() {
        // A line can wrap onto several rows.
        let height = line.size(row).height.max(row);
        if at.y < top + height || ix + 1 == lines.len() {
            let within =
                point((at.x - bounds.left()).max(px(0.)), (at.y - top).clamp(px(0.), (height - px(1.)).max(px(0.))));
            let (Ok(index) | Err(index)) = line.closest_index_for_position(within, row);
            return (start + index).min(layout.len());
        }
        top += height;
        // Past the line and the line break after it.
        start += line.len() + 1;
    }
    0
}

/// What's selected of a piece laid out in `layout`, which says `text`: `range` of it.
fn span(layout: &TextLayout, text: &str, range: Range<usize>) -> Span {
    let (bounds, row) = (layout.bounds(), layout.line_height());
    // GPUI has a place for every index from a text's start to its end, which `range` is within.
    let place = |index| layout.position_for_index(index).unwrap_or(bounds.origin);
    let mut start = place(range.start);
    if let Some(next) = text.get(range.clone()).and_then(|text| text.chars().next()).filter(|c| *c != '\n') {
        let after = place(range.start + next.len_utf8());
        if after.y > start.y {
            start = point(bounds.left(), after.y);
        }
    }
    Span { start, end: place(range.end), bounds, row }
}

/// What's selected of one piece, as it's painted: from `start` to `end`, the tops of the places at either end, in a
/// piece at `bounds`, whose rows are `row` tall.
struct Span {
    start: Point<Pixels>,
    end: Point<Pixels>,
    bounds: Bounds<Pixels>,
    row: Pixels,
}

/// The boxes that paint `spans`, what's selected of each piece from one end of a selection to the other, in order. One
/// that goes on into the next reaches the right edge of its last row, and the space down to the next is filled.
fn boxes(spans: &[Span]) -> Vec<Bounds<Pixels>> {
    let mut boxes = Vec::new();
    for (ix, span) in spans.iter().enumerate() {
        let next = spans.get(ix + 1);
        let end = if next.is_some() { point(span.bounds.right(), span.end.y) } else { span.end };
        boxes.extend(highlight(span.start, end, span.bounds, span.row));
        if let Some(next) = next {
            boxes.push(Bounds::from_corners(span.bounds.bottom_left(), point(span.bounds.right(), next.bounds.top())));
        }
    }
    boxes.retain(|bounds| bounds.size.height > Pixels::ZERO);
    boxes
}

/// Where the selection from `start` to `end` in a piece at `bounds` lies: across its rows, to its edges between them.
fn highlight(start: Point<Pixels>, end: Point<Pixels>, bounds: Bounds<Pixels>, row: Pixels) -> Vec<Bounds<Pixels>> {
    if start.y == end.y {
        return vec![Bounds::from_corners(start, Point::new(end.x, end.y + row))];
    }
    let first = Bounds::from_corners(start, Point::new(bounds.right(), start.y + row));
    let between = Bounds::from_corners(Point::new(bounds.left(), start.y + row), Point::new(bounds.right(), end.y));
    let last = Bounds::from_corners(Point::new(bounds.left(), end.y), Point::new(end.x, end.y + row));
    [first, between, last].into_iter().filter(|bounds| bounds.size.height > Pixels::ZERO).collect()
}

#[cfg(test)]
mod tests;
