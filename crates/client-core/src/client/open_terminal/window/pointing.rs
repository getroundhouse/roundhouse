//! Selecting with the pointer in a window, by the cell under it.

use protocol::terminal::{Numbering, RowNumber, TerminalPoint, TerminalRange};
use terminal_state::{Pointer, SelectBy};

use super::viewing::{Ask, Window};

impl Window {
    /// The cell under the pointer, by number, or the nearest one to it while it's past an edge, and how far across its
    /// cell it is, in thousandths, as Ghostty measures it.
    fn under(&self, at: Pointer) -> Option<(TerminalPoint, u32)> {
        if self.waiting() {
            return None;
        }
        let (top, screen) = (self.top()?, self.rows.screen.as_ref()?);
        let col = nearest(at.col, screen.size.cols);
        let row = top.plus(u64::from(nearest(at.row, screen.size.rows)));
        let across =
            (at.col.clamp(0., f32::from(screen.size.cols)) * 1000.).min(f32::from(screen.size.cols) * 1000. - 1.);
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to the grid first")]
        let across = (across as u32) % 1000;
        Some((TerminalPoint { row, col }, across))
    }

    pub(in crate::client::open_terminal) fn press(&mut self, at: Pointer, by: SelectBy) -> Vec<Ask> {
        let (Some((point, across)), Some(screen)) = (self.under(at), &self.rows.screen) else { return Vec::new() };
        let numbering = screen.numbering;
        self.changed = true;
        self.unkept(false, true);
        self.selecting
            .press(point, across, by)
            .map(|(of, step)| Ask::Bounds { numbering, at: point, of, step })
            .into_iter()
            .collect()
    }

    pub(in crate::client::open_terminal) fn drag(&mut self, at: Pointer) -> Vec<Ask> {
        let (Some((point, across)), Some(screen)) = (self.under(at), &self.rows.screen) else { return Vec::new() };
        let (numbering, cols, end) = (screen.numbering, screen.size.cols, screen.end());
        self.changed = true;
        self.unkept(false, true);
        let bounds = self.selecting.drag(point, across, cols, end);
        bounds.map(|(of, step)| Ask::Bounds { numbering, at: point, of, step }).into_iter().collect()
    }

    pub(in crate::client::open_terminal) fn release(&mut self) {
        self.selecting.release();
    }

    /// Selects everything, from the oldest row the terminal keeps to the end of the screen.
    pub(in crate::client::open_terminal) fn select_all(&mut self) {
        if self.waiting() {
            return;
        }
        self.unkept(false, true);
        let Some(screen) = &self.rows.screen else { return };
        let to = TerminalPoint { row: RowNumber::new(screen.end().get().saturating_sub(1)), col: screen.size.cols - 1 };
        self.selecting.all(TerminalRange { from: TerminalPoint { row: screen.first, col: 0 }, to });
        self.changed = true;
    }

    /// Whether what's selected is still to be settled, by a word's or a line's ends or a fit's reply on their way, so
    /// a copy asked now waits for them, as a person who double-clicks and copies at once means the word.
    pub(in crate::client::open_terminal) fn selection_coming(&self) -> bool {
        self.waiting() || self.selecting.coming()
    }

    /// What's selected, by number, with the numbering it's in, for its text to be copied.
    /// None while the view waits for a fit's reply, since its ends are in the numbering before.
    pub(in crate::client::open_terminal) fn selected(&self) -> Option<(Numbering, TerminalRange)> {
        if self.waiting() {
            return None;
        }
        Some((self.rows.screen.as_ref()?.numbering, self.selecting.range()?))
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to the cells there are first")]
fn nearest(at: f32, cells: u16) -> u16 {
    at.floor().clamp(0., f32::from(cells.saturating_sub(1))) as u16
}
