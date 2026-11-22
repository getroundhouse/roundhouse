//! What one chat knows of the references its texts name, and which it shows: those drawn in sight in its last frame,
//! while what draws them is shown.

use std::collections::HashMap;

use protocol::model::{ReferenceAsk, ReferenceRead};

use crate::linked_images::Waiter;

/// What's known of each reference, by what it asks, and which are drawn in the frame being drawn, those of which in
/// sight are what's shown once it's drawn.
pub(super) struct Book<W> {
    known: HashMap<ReferenceAsk, Known<W>>,
    /// Those drawn in this frame, in the order they were, by what drew them.
    this_frame: Vec<(ReferenceAsk, W)>,
    /// Whether this frame's end is already to be seen to.
    pub(super) settling: bool,
    /// What it last said it shows.
    pub(super) shown: Vec<ReferenceAsk>,
    /// Whether what draws them is hidden, as a chat is behind another's tab, so it shows none, whatever it draws.
    pub(super) hidden: bool,
}

struct Known<W> {
    /// What the hub said, as it was last drawn, or `None` until it has.
    read: Option<ReferenceRead>,
    /// What's drawn it since it was last told it changed.
    drawn_by: Vec<W>,
}

impl<W: Waiter> Book<W> {
    pub(super) fn new() -> Self {
        Self { known: HashMap::new(), this_frame: Vec::new(), settling: false, shown: Vec::new(), hidden: false }
    }

    /// Takes `ask` to be drawn by `drawer` in this frame, with what the hub said of it, `said`, which is kept unless
    /// it's nothing, since what the hub sent stays until it's no longer shown. Returns what's known of it, and
    /// whether this frame's end is yet to be seen to.
    pub(super) fn drawn(
        &mut self,
        ask: &ReferenceAsk,
        drawer: W,
        said: Option<ReferenceRead>,
    ) -> (Option<ReferenceRead>, bool) {
        let known = self.known.entry(ask.clone()).or_insert_with(|| Known { read: None, drawn_by: Vec::new() });
        if !known.drawn_by.contains(&drawer) {
            known.drawn_by.push(drawer);
        }
        if said.is_some() {
            known.read = said;
        }
        let read = known.read.clone();
        if !self.this_frame.iter().any(|(drawn, by)| drawn == ask && *by == drawer) {
            self.this_frame.push((ask.clone(), drawer));
        }
        (read, !std::mem::replace(&mut self.settling, true))
    }

    /// Ends this frame: what it drew in sight, as `in_sight` says of what drew each, is what's shown from now on, or
    /// none while it's hidden. Returns that, if it isn't what was shown, in whatever order.
    pub(super) fn settled(&mut self, in_sight: impl Fn(&W) -> bool) -> Option<Vec<ReferenceAsk>> {
        self.settling = false;
        let mut drawn = Vec::new();
        for (ask, by) in std::mem::take(&mut self.this_frame) {
            if !self.hidden && in_sight(&by) && !drawn.contains(&ask) {
                drawn.push(ask);
            }
        }
        if drawn.len() == self.shown.len() && drawn.iter().all(|ask| self.shown.contains(ask)) {
            return None;
        }
        self.shown.clone_from(&drawn);
        Some(drawn)
    }

    /// Takes in what the hub says of each now, `said`, and returns what drew those that changed, each once.
    pub(super) fn heard(&mut self, said: impl Fn(&ReferenceAsk) -> Option<ReferenceRead>) -> Vec<W> {
        let mut drawers = Vec::new();
        for (ask, known) in &mut self.known {
            let Some(now) = said(ask) else { continue };
            if known.read.as_ref() == Some(&now) {
                continue;
            }
            known.read = Some(now);
            for drawer in std::mem::take(&mut known.drawn_by) {
                if !drawers.contains(&drawer) {
                    drawers.push(drawer);
                }
            }
        }
        drawers
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::{GithubRepositoryName, IssueNumber};

    use super::*;

    fn ask(number: u64) -> ReferenceAsk {
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(number).unwrap() }
    }

    fn found(title: &str) -> ReferenceRead {
        ReferenceRead::Found { reference: Box::new(protocol::fixtures::issue("o/r", 12, title)) }
    }

    /// What a frame draws is what's shown once it's drawn, and only when that changes is it said again.
    #[test]
    fn what_a_frame_draws_is_what_is_shown() {
        let mut book = Book::<u32>::new();
        assert!(book.drawn(&ask(12), 1, None).1, "the frame's first");
        assert!(!book.drawn(&ask(13), 2, None).1);
        assert_eq!(book.settled(|_| true), Some(vec![ask(12), ask(13)]));
        book.drawn(&ask(13), 2, None);
        book.drawn(&ask(12), 1, None);
        assert_eq!(book.settled(|_| true), None, "the same again, in another order");
        book.drawn(&ask(13), 2, None);
        assert_eq!(book.settled(|_| true), Some(vec![ask(13)]), "#12 out of view");
        assert_eq!(book.settled(|_| true), Some(Vec::new()), "a frame that draws none");
    }

    /// What's drawn out of sight, as a row is to be measured, isn't shown, and nor is anything while what draws them
    /// is hidden, until it's shown again.
    #[test]
    fn only_what_is_drawn_in_sight_while_shown_is_shown() {
        let mut book = Book::<u32>::new();
        book.drawn(&ask(12), 1, None);
        book.drawn(&ask(13), 2, None);
        book.drawn(&ask(12), 2, None);
        assert_eq!(book.settled(|by| *by == 2), Some(vec![ask(13), ask(12)]), "#12 drawn in sight too");
        book.drawn(&ask(12), 1, None);
        assert_eq!(book.settled(|by| *by == 2), Some(Vec::new()), "drawn only out of sight");

        book.drawn(&ask(12), 2, None);
        book.hidden = true;
        assert_eq!(book.settled(|_| true), None, "none while hidden");
        book.hidden = false;
        book.drawn(&ask(12), 2, None);
        assert_eq!(book.settled(|_| true), Some(vec![ask(12)]));
    }

    /// What drew one is told when the hub's word on it changes, and not when it doesn't, and what the hub said is kept
    /// once it no longer says, as after it's no longer shown.
    #[test]
    fn what_drew_one_is_told_when_the_hub_word_on_it_changes() {
        let mut book = Book::<u32>::new();
        book.drawn(&ask(12), 1, None);
        book.drawn(&ask(12), 2, None);

        assert_eq!(book.heard(|_| Some(found("A"))), [1, 2]);
        assert_eq!(book.heard(|_| Some(found("A"))), Vec::<u32>::new(), "the same");
        assert_eq!(book.drawn(&ask(12), 3, None).0, Some(found("A")), "kept while the hub says nothing of it");
        assert_eq!(book.heard(|_| Some(found("B"))), [3], "those that drew it since it was last told");
    }
}
