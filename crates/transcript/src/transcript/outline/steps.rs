//! Stepping the chat between the lines the outline lists, a prompt, a compaction or an answer to a proposal, with
//! `Option-Up` and `Option-Down`, which put each where clicking it in the list does: at the bottom of the chat, with
//! what came before it above. A step goes from the line kept at the bottom, if a step or a click put one there, or
//! else from the end of the chat, or from the row at the bottom, where the chat was scrolled to. So a step back goes
//! first to the line being read, if it isn't at the bottom already, and from the end to the last line, and a step on
//! from the last line goes to the end.
//!
//! With Shift, they move the end of what's selected to the start of the prompt before it, or of the one whose turn
//! it's in, and to the start of the one after, so a turn can be selected from its prompt with one key. Like the other
//! keys that change what's selected, they go no further than the chat has loaded around what's selected. They pass
//! over a prompt with no words, since it draws no text to start at.

use super::super::Transcript;
use super::super::drawing::row_of;
use super::Line;
use crate::list::Reading;
use crate::selection::Way;
use crate::step_keys::Steps;
use crate::stops::stop_beside;
use crate::texts::{Of, TextAt};
use client_core::session_log::Row;
use gpui_kit::{Context, Window};
use std::ops::Range;

/// Where a step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// The line in this row, to the bottom.
    To(usize),
    /// The end of the chat.
    End,
}

/// The rows of the prompts a step with Shift from `row` stops at, of the outline's `lines`: those in the run of rows
/// `loaded` around it, since the keys that change what's selected go no further than the chat has loaded, that have
/// words, since one that only carries files draws no text to start at. The list's `tail`, its last row, draws what's
/// past the log's rows, such as a reply still coming in, so it goes with the run that reaches it.
pub(super) fn selecting_stops(lines: &[Line], loaded: &[Range<usize>], tail: usize, row: usize) -> Vec<usize> {
    let around = |run: &&Range<usize>| run.contains(&row) || (row == tail && run.end == tail);
    let Some(run) = loaded.iter().find(around) else { return Vec::new() };
    let has_words = |line: &&Line| line.prompt().is_some_and(|prompt| !prompt.text().is_empty());
    lines.iter().filter(|line| run.contains(&line.row)).filter(has_words).map(|line| line.row).collect()
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
    /// Takes the chat to `to`, as a step or a click in the outline's list does. What was being brought into view,
    /// such as Find's match, gives way, as it does to the keys that scroll a page, rather than scroll back to it after.
    pub(super) fn jump(&mut self, to: Step, cx: &mut Context<Self>) {
        self.selection.reveal().done();
        match to {
            Step::To(row) => self.list.go_to_row_at_bottom(row),
            Step::End => self.list.go_to_latest(),
        }
        cx.notify();
    }
}

impl Steps for Transcript {
    /// Steps the chat `way` to the line the outline lists before the one being read or after it, or on from the last
    /// to its end.
    fn step(&mut self, way: Way, cx: &mut Context<Self>) {
        let lines = self.outline.read(&self.log, &self.model.borrow().proposals_by_id);
        if let Some(to) = step(&lines, self.list.reading(), way) {
            self.jump(to, cx);
        }
    }

    /// Moves the end of what's selected `way` to the start of a prompt, the one whose turn it's in being the one it's
    /// in.
    fn select_step(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        let Some((end, within)) = self.selection.far_end() else { return };
        let Some(row) = row_of(&self.log, self.list.row_count(), end.of) else { return };
        let lines = self.outline.read(&self.log, &self.model.borrow().proposals_by_id);
        let tail = self.list.row_count().saturating_sub(1);
        let prompts = selecting_stops(&lines, &self.log.loaded_rows(), tail, row);
        // A prompt's text is drawn in its row, by that row's event, which is loaded.
        if let Some(prompt) = stop_beside(prompts, row, within, way)
            && let Row::Event(event) = self.log.row(prompt)
        {
            self.selection.move_to_start_of(TextAt::first_of(Of::Event(event.seq)), window, cx);
        }
    }
}
