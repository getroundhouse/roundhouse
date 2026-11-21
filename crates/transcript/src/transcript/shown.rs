//! Whether a transcript is shown, and which of its rows are in sight, which the issues and pull requests it shows of
//! GitHub go by.

use gpui_kit::Context;
use protocol::model::Seq;

use super::Transcript;

impl Transcript {
    /// Whether any of the row `seq` is drawn in showed in the last frame, rather than being drawn out of sight only to be
    /// measured.
    pub(crate) fn in_sight(&self, seq: Seq) -> bool {
        self.log.row_drawing(seq).is_some_and(|row| self.list.shows(row))
    }

    /// Says whether it's shown, as its chat's tab is, so the issues and pull requests it names are what the app shows
    /// of GitHub only while it is.
    pub fn set_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        if self.references.set_shown(shown) {
            // What it draws next is what it shows.
            cx.notify();
        }
    }
}
