//! Which rows are in the window, and where, worked out from one row that stays where it was in the last frame. A
//! row's place comes from the rows laid out around it, so the rows nobody is looking at are never measured for it. A
//! row is drawn to show it, or to find a height that isn't known. One out of the window whose height is known is only
//! given its place.

use std::collections::VecDeque;

use gpui_kit::{Pixels, px};

use super::state::Place;

/// How far past each end of the window rows are laid out, so one that hasn't been fetched is asked for before it's in
/// sight, and one whose height is out of date has it before it scrolls in.
pub(super) const OVERDRAW: Pixels = px(400.);

/// What laying rows out asks of them.
pub(super) trait Rows<T> {
    /// How tall the row is known to be, as the list is now. `None` for a row that has to be drawn to find out:
    /// one never measured, or not since it changed or the list did.
    fn kept(&self, row: usize) -> Option<Pixels>;
    /// Draws the row, and says how tall it came out. `None` for a row that hasn't been fetched.
    fn draw(&mut self, row: usize) -> Option<(Pixels, T)>;
    /// How tall a row that hasn't been fetched is taken to be.
    fn guess(&self, row: usize) -> Pixels;
}

/// A row with a place in the window.
pub(super) struct LaidRow<T> {
    pub(super) row: usize,
    /// How far below the window's top its own top is.
    pub(super) top: Pixels,
    pub(super) height: Pixels,
    /// The row as drawn, for each row that shows in the window. Any other has none: one out of the window, one of
    /// no height, and one that hasn't been fetched.
    pub(super) drawn: Option<T>,
    /// Whether it's been fetched. One that hasn't takes the room it's guessed to.
    pub(super) fetched: bool,
}

impl<T> LaidRow<T> {
    pub(super) fn bottom(&self) -> Pixels {
        self.top + self.height
    }

    /// Whether any of it would show in a window `tall`.
    fn shows(&self, tall: Pixels) -> bool {
        shows(self.top, self.height, tall)
    }
}

fn shows(top: Pixels, height: Pixels, tall: Pixels) -> bool {
    height > px(0.) && top < tall && top + height > px(0.)
}

/// Where to lay the rows out from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Start {
    /// The latest row, with its end at the window's bottom.
    Latest,
    /// The place that was at the window's top, and how far down the session the top has moved from it since. The
    /// wheel moves it without knowing how tall the rows it passes are, and laying them out finds which row that
    /// comes to. If the row is no longer as tall as the place is far into it, the place is the row's end.
    Top { at: Place, moved: Pixels },
    /// A row that stays where it was in the window, and how far down the session the window has moved since, as
    /// for [`Start::Top`]. The rows above it are laid out up from it, so one that's changed height grows or shrinks
    /// up, past the window's top, and moves nothing below. If the row is now too short to reach down into the window
    /// from where its top was, its end is at the window's top.
    Anchored { at: Anchor, moved: Pixels },
}

/// The row the list keeps where it is in the window from one frame to the next, so what's on screen stays still
/// when a row in sight changes height by itself, as a long text does when it's read or an image when it's decoded.
///
/// It's the first row that shows whose top is in the window. A row that reaches up past the window's top can be
/// changing at either end, so it's the anchor only when no other row starts in the window, which is when it fills
/// it, and then the list stays as far into it as it was. Someone who opens or closes something in a row makes that
/// row the anchor, so the line they clicked stays under the pointer while what it opened to grows under it. The
/// wheel, or anything else that moves the list, chooses again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Anchor {
    pub(super) row: usize,
    /// How far below the window's top the row's top is, which is less than nothing for one reaching up past it.
    pub(super) top: Pixels,
}

/// The rows in the window, and a little past each end of it, in order.
pub(super) struct Layout<T> {
    pub(super) rows: VecDeque<LaidRow<T>>,
    /// The row that came out at the window's top, and how far into it.
    pub(super) top: Place,
    /// The row to keep where it is in the next frame, and where it came out.
    pub(super) anchor: Option<Anchor>,
    /// Whether the latest row ends in the window, so there's nothing further down to scroll to.
    pub(super) end_in_sight: bool,
}

/// Lays out a session of `rows` rows in a window `tall`.
pub(super) fn lay_out<T>(rows: usize, start: Start, tall: Pixels, of: &mut impl Rows<T>) -> Layout<T> {
    let mut laid: VecDeque<LaidRow<T>> = VecDeque::new();
    let Some(last) = rows.checked_sub(1) else {
        return Layout { rows: laid, top: Place::default(), anchor: None, end_in_sight: true };
    };
    let mut place = |row: usize, top_of: &dyn Fn(Pixels) -> Pixels| {
        // A row whose height is known, and which wouldn't show at it, has its place without being drawn.
        if let Some(height) = of.kept(row).filter(|height| !shows(top_of(*height), *height, tall)) {
            return LaidRow { row, top: top_of(height), height, drawn: None, fetched: true };
        }
        if let Some((height, drawn)) = of.draw(row) {
            return LaidRow { row, top: top_of(height), height, drawn: Some(drawn), fetched: true };
        }
        let height = of.guess(row);
        LaidRow { row, top: top_of(height), height, drawn: None, fetched: false }
    };
    laid.push_back(match start {
        Start::Latest => place(last, &|height| tall - height),
        Start::Top { at, moved } => place(at.row.min(last), &|height| -at.offset.min(height) - moved),
        Start::Anchored { at, moved } => place(at.row.min(last), &|height| at.top.max(-height) - moved),
    });
    fill_up(&mut laid, px(0.), &mut place);
    fill_down(&mut laid, last, tall + OVERDRAW, &mut place);
    // The rows ended short of the window's bottom, so the last of them goes there, and more show above.
    let short = tall - laid.back().map_or(tall, LaidRow::bottom);
    if short > px(0.) {
        for row in &mut laid {
            row.top += short;
        }
        fill_up(&mut laid, px(0.), &mut place);
    }
    fill_up(&mut laid, -OVERDRAW, &mut place);
    show(&mut laid, tall, of);
    let top = laid.iter().find(|row| row.bottom() > px(0.)).or(laid.back());
    let top = top.map_or(Place::default(), |at| Place { row: at.row, offset: (-at.top).max(px(0.)) });
    let end_in_sight = laid.back().is_some_and(|row| row.row == last && row.bottom() <= tall + px(1.));
    let anchor = anchor(&laid, start, top, tall);
    Layout { rows: laid, top, anchor, end_in_sight }
}

/// The row to keep where it is in the next frame, as [`Anchor`] says: the one this frame kept, while nothing has
/// moved the list and it's still in sight, or else the first row that shows whose top is in the window, or else the
/// row at the top.
fn anchor<T>(laid: &VecDeque<LaidRow<T>>, start: Start, top: Place, tall: Pixels) -> Option<Anchor> {
    let at = |row: &LaidRow<T>| Anchor { row: row.row, top: row.top };
    let kept = match start {
        Start::Anchored { at: kept, moved } if moved == px(0.) => {
            laid.iter().find(|row| row.row == kept.row && row.shows(tall)).map(at)
        }
        Start::Latest | Start::Top { .. } | Start::Anchored { .. } => None,
    };
    let first_in_sight = || laid.iter().find(|row| row.top >= px(0.) && row.shows(tall)).map(at);
    let at_the_top = || laid.iter().find(|row| row.row == top.row).map(|_| Anchor { row: top.row, top: -top.offset });
    kept.or_else(first_in_sight).or_else(at_the_top)
}

/// Gives a row its place, which is its top, given how tall it is.
type PlaceRow<'a, T> = dyn FnMut(usize, &dyn Fn(Pixels) -> Pixels) -> LaidRow<T> + 'a;

/// Lays out the rows before the first until one reaches up to `to`, or there are no more. If the first row of the
/// session then starts below the window's top, everything moves up to put it there.
fn fill_up<T>(laid: &mut VecDeque<LaidRow<T>>, to: Pixels, place: &mut PlaceRow<'_, T>) {
    while let Some(first) = laid.front().filter(|first| first.top > to && first.row > 0) {
        let (row, below) = (first.row - 1, first.top);
        laid.push_front(place(row, &|height| below - height));
    }
    let gap = laid.front().map_or(px(0.), |first| first.top);
    if gap > px(0.) {
        for row in laid.iter_mut() {
            row.top -= gap;
        }
    }
}

/// Lays out the rows after the last until one reaches down to `to`, or there are no more.
fn fill_down<T>(laid: &mut VecDeque<LaidRow<T>>, last: usize, to: Pixels, place: &mut PlaceRow<'_, T>) {
    while let Some(end) = laid.back().filter(|end| end.bottom() < to && end.row < last) {
        let (row, top) = (end.row + 1, end.bottom());
        laid.push_back(place(row, &|_| top));
    }
}

/// Leaves each row that shows in the window drawn, and no other. A row can have been given its place out of the
/// window and then moved into it, when the rows stopped at an end of the session. If it comes out another height
/// than was known, the rows after it move by the difference.
fn show<T>(laid: &mut VecDeque<LaidRow<T>>, tall: Pixels, of: &mut impl Rows<T>) {
    let mut moved = px(0.);
    for row in laid.iter_mut() {
        row.top += moved;
        if !row.shows(tall) {
            row.drawn = None;
        } else if row.drawn.is_none() && row.fetched {
            match of.draw(row.row) {
                Some((height, drawn)) => {
                    moved += height - row.height;
                    (row.height, row.drawn) = (height, Some(drawn));
                }
                None => row.fetched = false,
            }
        }
    }
}

#[cfg(test)]
mod tests;
