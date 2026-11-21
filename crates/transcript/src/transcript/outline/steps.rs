//! Stepping the chat between the lines the outline lists, a prompt, a compaction or an answer to a proposal, with
//! `Option-Up` and `Option-Down`, which put each where clicking it in the list does: at the bottom of the chat, with
//! what came before it above. A step goes from the line kept at the bottom, if a step or a click put one there, or
//! else from the end of the chat, or from the row at the bottom, where the chat was scrolled to. So a step back goes
//! first to the line being read, if it isn't at the bottom already, and from the end to the last line, and a step on
//! from the last line goes to the end.

use super::super::Transcript;
use super::Line;
use crate::list::Reading;
use crate::selection::Way;
use gpui_kit::Context;

/// Where a step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// The line in this row, to the bottom.
    To(usize),
    /// The end of the chat.
    End,
}

/// Where a step `way` from `reading` goes, among the outline's `lines`, if anywhere.
pub(super) fn step(lines: &[Line], reading: Reading, way: Way) -> Option<Step> {
    let mut rows = lines.iter().map(|line| line.row);
    match (reading, way) {
        (Reading::Kept(kept), Way::Back) => rows.rfind(|row| *row < kept).map(Step::To),
        (Reading::Scrolled(bottom), Way::Back) => rows.rfind(|row| *row <= bottom).map(Step::To),
        (Reading::End, Way::Back) => rows.next_back().map(Step::To),
        (Reading::Kept(row) | Reading::Scrolled(row), Way::On) => {
            Some(rows.find(|on| *on > row).map_or(Step::End, Step::To))
        }
        (Reading::End, Way::On) => None,
    }
}

impl Transcript {
    /// Steps the chat `way` to the line the outline lists before the one being read or after it, or on from the last
    /// to its end, as `Option-Up` and `Option-Down` do.
    pub fn step(&mut self, way: Way, cx: &mut Context<Self>) {
        let lines = self.outline.read(&self.log, &self.model.borrow().proposals_by_id);
        match step(&lines, self.list.reading(), way) {
            Some(Step::To(row)) => self.list.go_to_row_at_bottom(row),
            Some(Step::End) => self.list.go_to_latest(),
            None => return,
        }
        cx.notify();
    }
}
