//! A revision the agent writes while the person may be reading the one before it is held back until they ask for it.
//! While the tab is in sight, or has something chosen or written in it that isn't sent, the page stays on the revision
//! it shows, with what's chosen and written, and a banner over it says the newer one is there, with a button that
//! shows it. Nothing can be sent meanwhile, since the hub refuses a response to a revision the agent has replaced.
//! Showing the newest takes the note along, and what's chosen for each question it still asks, where the option chosen
//! is still there under its letter, since the agent keeps a letter only for an option it keeps. Otherwise the newest
//! takes the page at once: behind another tab with nothing unsent, nobody is reading it, and once the revision shown
//! is responded to, its bar or its banner has said so.

use std::sync::Arc;

use gpui_kit::Context;
use protocol::model::{OptionPlace, Proposal, ProposalQuestion, Proposed, RevisionNumber};

use super::answering::Answering;
use super::view::ProposalView;

impl ProposalView {
    /// Takes in a newer copy of its record: the revision shown as it stands now, or a newer revision, which takes the
    /// page unless the one shown is held, as the module says.
    pub fn set_proposal(&mut self, proposal: Arc<Proposal>, cx: &mut Context<Self>) {
        if Arc::ptr_eq(self.newest.as_ref().unwrap_or(&self.proposal), &proposal) {
            return;
        }
        if proposal.current.number == self.proposal.current.number {
            self.newest = None;
            self.proposal = proposal;
        } else if self.waits() && (self.in_sight || self.has_unsent(cx)) {
            self.newest = Some(proposal);
        } else {
            self.go_on_to(proposal, cx);
        }
        // Once it's answered, as from another client, there's no note to dictate into.
        self.follow_dictation(cx);
        cx.notify();
    }

    /// The newest revision's number, while it's held back.
    pub fn held_back(&self) -> Option<RevisionNumber> {
        self.newest.as_ref().map(|newest| newest.current.number)
    }

    /// Shows the newest revision, if it's held back, as its banner's button asks, and Open on one of the proposal's
    /// lines in the chat. While a response is on its way the page stays, until the hub says whether it kept it.
    pub fn show_newest(&mut self, cx: &mut Context<Self>) {
        if self.answering.going() {
            return;
        }
        let Some(newest) = self.newest.take() else { return };
        self.go_on_to(newest, cx);
        self.follow_dictation(cx);
        cx.notify();
    }

    /// The hub has kept a response to the revision shown, so a newer one held back was written after it: what was
    /// chosen and written is sent, and the newest takes the page with none of it.
    pub(super) fn sent_before_a_revision(&mut self, cx: &mut Context<Self>) {
        let Some(newest) = self.newest.take() else { return };
        self.let_go(cx);
        self.proposal = newest;
        self.scroll_to_top();
    }

    /// Puts `newest` on the page, from its top. What's chosen and written goes on to it while the revision shown still
    /// waits for the person, and is let go once that one is responded to or withdrawn, since it was sent or can't be.
    fn go_on_to(&mut self, newest: Arc<Proposal>, cx: &mut Context<Self>) {
        if self.waits() {
            let chosen = carried(&self.answering.chosen, &self.proposal.current.proposed, &newest.current.proposed);
            self.answering.carry_to(chosen);
        } else {
            self.let_go(cx);
        }
        self.newest = None;
        self.proposal = newest;
        self.scroll_to_top();
    }

    /// Ends the note's recording, and starts afresh with nothing chosen or written.
    fn let_go(&mut self, cx: &mut Context<Self>) {
        self.end_dictation(cx);
        self.answering = Answering::default();
    }
}

/// What's `chosen` in `from` that `to` still offers, for each of `to`'s questions: the option under the letter of the
/// one chosen for the question at the same place in `from`, when that question has the same header. A question that's
/// moved or renamed starts with nothing chosen, rather than with another question's choice.
pub(super) fn carried(chosen: &[Option<OptionPlace>], from: &Proposed, to: &Proposed) -> Vec<Option<OptionPlace>> {
    let still_offered = |(at, question): (usize, &ProposalQuestion)| {
        let before = from.questions.get(at).filter(|before| before.header.trim() == question.header.trim())?;
        let letter = before.letter(chosen.get(at).copied().flatten()?)?;
        question.place_of(letter)
    };
    to.questions.iter().enumerate().map(still_offered).collect()
}
