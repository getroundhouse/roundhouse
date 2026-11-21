//! Finding the line above or below the end of what's selected, and the place on it nearest across, as `lines` says.

use gpui_kit::{Bounds, Pixels, point, px, size};

use super::super::super::geometry::SAME_ROW;
use super::super::super::{Drawn, End, Geometry, Selecting, Selection};
use super::super::{Tried, Way, character_beside};
use crate::texts::{Leaves, TextKey};

/// A run of moves up and down.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::selection::keys) struct Run {
    /// The end it left, which a move from anywhere else starts another run from.
    pub(in crate::selection::keys) end: End,
    /// Where across it keeps to, which is where the end was when the first of them started.
    pub(super) across: Pixels,
    /// Whether the end is at the start of a row, where the row before wraps onto it, rather than at the end of the
    /// row before, which is where GPUI draws it.
    pub(super) wrapped: bool,
}

/// Where a move up or down starts from.
struct Start {
    /// Where the end is drawn, as a caret there would be.
    caret: Bounds<Pixels>,
    /// Where across it goes to the place nearest.
    across: Pixels,
    /// The block of rows the end is in, if it's drawn, whose neighbours beside it aren't above or below it.
    own: Option<Bounds<Pixels>>,
    /// The text the end is in, if it's drawn, all of which is laid out to go to.
    text: Option<TextKey>,
}

/// What came of a move up or down.
#[derive(Debug, PartialEq)]
pub(super) enum Line {
    /// To the end the run it's in left.
    To(Run),
    /// Nowhere yet, with no line that way among the texts drawn, and where across the run keeps to, for once there's
    /// one.
    Past(Option<Run>),
    /// Nowhere, since the end isn't drawn.
    NotDrawn,
}

impl Selection {
    /// Tries moving `end` to the line `way` of it, in a run of moves up and down, as [`move_a_line`] says.
    pub(in crate::selection::keys) fn line_beside(&self, end: &End, way: Way, past: bool) -> Tried {
        let selecting = self.inner.borrow();
        let run = selecting.keyed.run.as_ref().filter(|run| run.end == *end);
        let moved = move_a_line(&selecting.drawn, &|text| self.texts.leaves_of(text), end, run, way, past);
        drop(selecting);
        match moved {
            Line::To(run) => Tried::Moved(run.end.clone(), Some(run)),
            Line::Past(run) => {
                self.inner.borrow_mut().keyed.run = run;
                Tried::Past(way)
            }
            Line::NotDrawn => Tried::NotDrawn,
        }
    }

    /// Where `end` is drawn in the last frame, as a caret there would be, and the text it's in, if that's drawn: on the
    /// row a run of moves up and down left it on, where a row wraps.
    pub(in crate::selection::keys) fn end_drawn<'a>(
        &self,
        selecting: &'a Selecting,
        end: &End,
    ) -> Option<(Bounds<Pixels>, &'a Drawn)> {
        let wrapped = selecting.keyed.run.as_ref().is_some_and(|run| run.end == *end && run.wrapped);
        let (caret, drawn, _) = drawn_at(&selecting.drawn, &|text| self.texts.leaves_of(text), end, wrapped)?;
        Some((caret, drawn))
    }
}

/// Where a move up or down from `end` goes, `way`, in `run` if it's in one, among the texts `drawn`, which show what
/// `leaves` says. The end has to be drawn, but once the list has gone `past` it for this move, so it's no longer drawn,
/// it's before every text drawn, and the line is the first of them that way.
pub(super) fn move_a_line(
    drawn: &[Drawn],
    leaves: &dyn Fn(&TextKey) -> Option<Leaves>,
    end: &End,
    run: Option<&Run>,
    way: Way,
    past: bool,
) -> Line {
    let wrapped = run.is_some_and(|run| run.wrapped);
    let start = match drawn_at(drawn, leaves, end, wrapped) {
        Some((caret, _, geometry)) => {
            let own = geometry.blocks().into_iter().find(|block| holds(*block, caret));
            Start { caret, across: run.map_or(caret.left(), |run| run.across), own, text: Some(end.text.clone()) }
        }
        None if past => {
            let Some(caret) = before_every(drawn, way) else { return Line::Past(run.cloned()) };
            Start { caret, across: run.map_or(caret.left(), |run| run.across), own: None, text: None }
        }
        None => return Line::NotDrawn,
    };
    match nearest_line(drawn, leaves, &start, way) {
        Some((moved, wrapped)) => Line::To(Run { end: moved, across: start.across, wrapped }),
        None => Line::Past(Some(Run { end: end.clone(), across: start.across, wrapped })),
    }
}

/// Where `end` is drawn among the texts `drawn`, which show what `leaves` says, as a caret there would be, with the
/// text and the leaf it's in, if it's drawn: at the start of the row after where a row wraps, if it's `wrapped` there.
fn drawn_at<'a>(
    drawn: &'a [Drawn],
    leaves: &dyn Fn(&TextKey) -> Option<Leaves>,
    end: &End,
    wrapped: bool,
) -> Option<(Bounds<Pixels>, &'a Drawn, &'a Geometry)> {
    let text = drawn.iter().find(|drawn| drawn.text == end.text)?;
    let (_, geometry) = text.leaves.iter().find(|(leaf, _)| *leaf == end.leaf)?;
    let on_the_next_row = || caret_after_wrap(leaves(&end.text)?.leaf(end.leaf)?, geometry, end.byte);
    let caret = wrapped.then(on_the_next_row).flatten().unwrap_or_else(|| geometry.caret(end.byte));
    Some((caret, text, geometry))
}

/// Where a move up or down from `from` goes, `way`, among the texts `drawn`, which show what `leaves` says: the place
/// nearest across on the row nearest that way, in the end's own text or what shows of another, and whether it's at the
/// start of a row that the one before wraps onto. Where two rows are as near, as cells in a table's row are, the one
/// across from it, or the nearest across to it.
fn nearest_line(
    drawn: &[Drawn],
    leaves: &dyn Fn(&TextKey) -> Option<Leaves>,
    from: &Start,
    way: Way,
) -> Option<(End, bool)> {
    let (caret, x) = (from.caret, from.across);
    // Every block of rows that way, with how far its nearest row that way is from the end's, how far it is across,
    // and the point on that row to find a place at.
    let mut rows = Vec::new();
    for text in drawn {
        let own = from.text.as_ref() == Some(&text.text);
        for (leaf, geometry) in &text.leaves {
            for block in geometry.blocks().into_iter().filter_map(|block| goes_to(block, own, text)) {
                if from.own.is_some_and(|own| beside(own, block)) {
                    continue;
                }
                let y = match way {
                    Way::Back if block.top() < caret.top() - SAME_ROW => {
                        (block.bottom().min(caret.top()) - px(1.)).max(block.top())
                    }
                    Way::On if block.bottom() > caret.bottom() + SAME_ROW => {
                        (block.top().max(caret.bottom()) + px(1.)).min(block.bottom() - SAME_ROW)
                    }
                    Way::Back | Way::On => continue,
                };
                let off = if x < block.left() {
                    block.left() - x
                } else if x > block.right() {
                    x - block.right()
                } else {
                    Pixels::ZERO
                };
                rows.push(((y - caret.center().y).abs(), off, y, text, *leaf, geometry));
            }
        }
    }
    let closest = rows.iter().map(|(far, ..)| *far).reduce(Pixels::min)?;
    // Within half a row of the nearest is as near, so a cell beside the one across from it, a little lower, isn't it.
    let as_near = rows.iter().filter(|(far, ..)| *far <= closest + caret.size.height / 2.);
    let (_, _, y, text, leaf, geometry) = as_near.min_by(|a, b| a.1.as_f32().total_cmp(&b.1.as_f32()))?;
    let found = geometry.index_for_position(point(x, *y), geometry.bounds());
    let shown = leaves(&text.text);
    let (byte, wrapped) = (shown.as_ref().and_then(|shown| shown.leaf(*leaf)))
        .map_or((found, false), |shown| nearest_across(shown, geometry, found, *y, x));
    Some((End { text: text.text.clone(), leaf: *leaf, byte }, wrapped))
}

/// What of `block`, a block of rows of `text`, a move can go to: all of it in the end's `own` text, and what shows of
/// it in another, if any does.
fn goes_to(block: Bounds<Pixels>, own: bool, text: &Drawn) -> Option<Bounds<Pixels>> {
    let block = if own { block } else { block.intersect(&text.bounds) };
    (block.size.height > Pixels::ZERO).then_some(block)
}

/// Where a caret would be just before what shows of every text `drawn`, `way` of them: above it going down, and below
/// it going up, a row as tall as the first of them has.
fn before_every(drawn: &[Drawn], way: Way) -> Option<Bounds<Pixels>> {
    let showing = drawn.iter().flat_map(|text| {
        let blocks =
            text.leaves.iter().flat_map(|(_, geometry)| geometry.blocks().into_iter().map(move |b| (b, geometry)));
        blocks.filter_map(move |(block, geometry)| Some((goes_to(block, false, text)?, geometry)))
    });
    let laid_out: Vec<(Bounds<Pixels>, &Geometry)> = showing.collect();
    let (first, row) = laid_out.first().map(|(block, geometry)| (*block, geometry.caret(0).size.height))?;
    let all = laid_out.iter().map(|(block, _)| *block).fold(first, |all, block| all.union(&block));
    let top = match way {
        Way::Back => all.bottom(),
        Way::On => all.top() - row,
    };
    Some(Bounds::new(point(all.left(), top), size(px(1.), row)))
}

/// Whether `block` is beside `own`, rather than above or below it: another block that's partly as high up, as a cell
/// beside another in a table's row is.
fn beside(own: Bounds<Pixels>, block: Bounds<Pixels>) -> bool {
    block != own && block.top() < own.bottom() && own.top() < block.bottom()
}

/// Whether `bounds` holds the row `caret` is on.
fn holds(bounds: Bounds<Pixels>, caret: Bounds<Pixels>) -> bool {
    bounds.top() <= caret.center().y && caret.center().y <= bounds.bottom()
}

/// Of the place `found` in `text`, which `geometry` lays out, and the places a character either side of it, the one on
/// the row at `y` nearest `x` across, and whether it's at the start of that row where the row before wraps onto it.
/// GPUI puts a point anywhere on a row's last character past it, which a press goes by, as the kit does, but a move up
/// or down is to the place nearest where it was across.
fn nearest_across(text: &str, geometry: &Geometry, found: usize, y: Pixels, x: Pixels) -> (usize, bool) {
    let found = text.floor_char_boundary(found.min(text.len()));
    let on_the_row = |caret: &Bounds<Pixels>| caret.top() <= y && y < caret.bottom();
    let on = |place: usize| {
        let caret = geometry.caret(place);
        if on_the_row(&caret) {
            return Some((caret, false));
        }
        caret_after_wrap(text, geometry, place).filter(on_the_row).map(|caret| (caret, true))
    };
    let off = |caret: &Bounds<Pixels>| (caret.left() - x).abs().as_f32();
    [character_beside(text, found, Way::Back), Some(found), character_beside(text, found, Way::On)]
        .into_iter()
        .flatten()
        .filter_map(|place| on(place).map(|(caret, wrapped)| (place, caret, wrapped)))
        .min_by(|(_, a, _), (_, b, _)| off(a).total_cmp(&off(b)))
        .map_or((found, false), |(place, _, wrapped)| (place, wrapped))
}

/// Where `place` in `text`, which `geometry` lays out, is drawn as the start of the row after the one GPUI draws it on,
/// if a row wraps there. A line break isn't where a row wraps, so the place before one is only at the end of its row.
fn caret_after_wrap(text: &str, geometry: &Geometry, place: usize) -> Option<Bounds<Pixels>> {
    let next = character_beside(text, place, Way::On).filter(|next| !text[place..*next].contains('\n'))?;
    geometry.caret_on_the_next_row(place, next)
}
