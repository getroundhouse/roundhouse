//! A row held still: what someone clicks to open, they can click again to close without moving the pointer. So the
//! row it's in stays exactly where it is in the window, whatever the list was keeping to, and even where closing
//! something in the session's last rows leaves it ending above the window's bottom. The list goes on keeping to the
//! latest row from there, as [`Start::Still`](crate::list::layout::Start::Still) says.

use super::{Keeping, State};
use crate::list::layout::Anchor;

impl State {
    /// Someone opened or closed something in `row`, which keeps it exactly where it is in the window, as
    /// [`Start::Still`](crate::list::layout::Start::Still) says, if it's in sight. It's kept still rather than with the session's end, or a row's, at
    /// the window's bottom, so what was clicked stays under the pointer, to click again.
    pub(in crate::list) fn keep_still(&mut self, row: usize) {
        let top = self.heights.above(row) - self.heights.above(self.top.row) - self.top.offset;
        if row >= self.top.row && top < self.tall() {
            self.anchor = Some(Anchor { row, top });
            self.keeping = Keeping::Still { follows: false };
        }
    }
}

#[cfg(test)]
mod tests;
