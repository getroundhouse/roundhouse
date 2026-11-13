//! The list scrolling down to the latest row by itself, over a moment rather than at once, as the button back to it
//! sends it. The latest row can move on the way, as more of the session arrives and as the rows passed turn out
//! other heights than they were taken to be. So rather than going to a place worked out as it set off, each frame
//! moves by a share of what it finds is left, as a `look::motion::Scrolling` says, and the list arrives at the latest
//! row wherever that has got to.

use gpui_kit::px;
use look::motion::Scrolling;

use super::{Keeping, State};

/// How far up the list can be and still slide all the way down, in windows' heights.
const SLIDES_UP_TO: f32 = 2.;
/// How far above the latest row a list further up than [`SLIDES_UP_TO`] goes straight to, in windows' heights, to
/// slide the rest. Sliding further would pass rows too quickly to follow, and load pages of history on the way that
/// nobody would see. A jump is a window or more, so it doesn't look like a stumble at the start of the slide.
const SLIDES_FROM: f32 = 1.;

impl State {
    /// Sets off down to the latest row, to keep to it once it's there.
    pub(in crate::list) fn head_to_latest(&mut self, scrolling: Scrolling) {
        if self.is_following() {
            return;
        }
        let (furthest, tall) = (self.furthest(), self.tall());
        if furthest - self.scrolled() > tall * SLIDES_UP_TO {
            self.go_to(furthest - tall * SLIDES_FROM);
        }
        self.keeping = Keeping::ToLatest(scrolling);
    }

    /// Whether it's on its way down to the latest row by itself.
    pub(in crate::list) fn is_heading_to_latest(&self) -> bool {
        matches!(self.keeping, Keeping::ToLatest(_))
    }

    /// Moves on toward the latest row, by all but the share of what's left that `move_on` says is still to go, given
    /// the scroll to move on. Once none is, it's there, and keeps to it.
    pub(in crate::list) fn head_on(&mut self, move_on: impl FnOnce(&mut Scrolling) -> f32) {
        let Keeping::ToLatest(scrolling) = &mut self.keeping else {
            return;
        };
        let share_to_go = move_on(scrolling);
        if share_to_go <= 0. {
            self.go_to_latest();
            return;
        }
        let to_go = (self.furthest() - self.down(&self.heights)).max(px(0.));
        self.moved += to_go * (1. - share_to_go);
    }
}

#[cfg(test)]
mod tests;
