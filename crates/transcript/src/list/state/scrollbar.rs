//! What the scrollbar goes by, and a drag of its thumb, which goes by the rows' heights as they were when it began.

use gpui_kit::{Pixels, px};

use super::{Keeping, Place, State};
use crate::list::heights::Heights;

/// Less than this isn't the scrollbar moving the list, only the same place read back and rounded.
const NO_MOVE: Pixels = px(0.01);

impl State {
    /// A drag of the scrollbar's thumb begins, which stops the list on its way down to the latest row, or keeping a
    /// row at the bottom.
    pub(in crate::list) fn start_drag(&mut self) {
        self.dragging = Some(self.heights.clone());
        match self.keeping {
            Keeping::ToLatest(_) | Keeping::AtBottom(_) => self.keeping = Keeping::Place,
            Keeping::Latest | Keeping::Place => {}
        }
    }

    pub(in crate::list) fn end_drag(&mut self) {
        self.dragging = None;
    }

    /// The heights the scrollbar goes by.
    fn for_the_bar(&self) -> &Heights {
        self.dragging.as_ref().unwrap_or(&self.heights)
    }

    /// How tall the scrollbar takes the session to be.
    pub(in crate::list) fn bar_tall(&self) -> Pixels {
        self.for_the_bar().total().max(self.tall())
    }

    /// How far down the scrollbar takes the session to be scrolled.
    pub(in crate::list) fn bar_scrolled(&self) -> Pixels {
        match &self.dragging {
            // Where the row at the top was when the drag began, so the thumb stays under the pointer.
            Some(heights) => {
                let furthest = (heights.total() - self.tall()).max(px(0.));
                self.down(heights).clamp(px(0.), furthest)
            }
            None => self.scrolled(),
        }
    }

    /// The scrollbar scrolls to `down`. A drag of the thumb goes to the row that was that far down when the drag
    /// began. Anything else moves by the difference from where it is, as the wheel does, so it passes the rows
    /// between at the heights they turn out to have. Either way, as far down as the scrollbar goes is the latest
    /// row, which is kept to again.
    pub(in crate::list) fn bar_scroll_to(&mut self, down: Pixels) {
        let furthest = if let Some(heights) = &self.dragging {
            let furthest = (heights.total() - self.tall()).max(px(0.));
            let (row, offset) = heights.row_at(down.clamp(px(0.), furthest));
            self.send_to(Place { row, offset });
            self.keeping = Keeping::Place;
            furthest
        } else {
            let turned = self.scrolled() - down;
            if turned.abs() > NO_MOVE {
                self.scroll_by(turned);
            }
            self.furthest()
        };
        if furthest > px(0.) && down >= furthest {
            self.go_to_latest();
        }
    }
}

#[cfg(test)]
mod tests;
