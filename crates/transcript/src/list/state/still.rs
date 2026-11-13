//! What opening or closing something in a row does to where the list is, as [`State::keep_still`] says.

use super::{Keeping, State};
use crate::list::layout::Anchor;

impl State {
    /// Someone opened something in `row`, or closed something in it if not `open`, which keeps the row where it is in
    /// the window, if it's in sight, so what was clicked stays under the pointer to click again. That puts the list at
    /// a place, whatever it was keeping to but for the one case below, and the next frame lays it out from the row, as
    /// [`Start::Anchored`](crate::list::layout::Start::Anchored) says. So opening something in the latest row leaves
    /// the session's end out of sight, as if someone had scrolled away from it, and closing something that leaves too
    /// little below the row to reach the window's bottom brings the rows down to it.
    ///
    /// Closing something while the list keeps to its latest row leaves it doing so. Closing only takes the session's
    /// end up, and the rows come down to put it back at the window's bottom either way, so holding the row would only
    /// stop the list following what arrives before the next frame, and count it as scrolled away for that frame.
    pub(in crate::list) fn keep_still(&mut self, row: usize, open: bool) {
        if !open && matches!(self.keeping, Keeping::Latest) {
            return;
        }
        let top = self.heights.above(row) - self.heights.above(self.top.row) - self.top.offset;
        if row >= self.top.row && top < self.tall() {
            self.anchor = Some(Anchor { row, top });
            self.keeping = Keeping::Place;
        }
    }
}

#[cfg(test)]
mod tests;
