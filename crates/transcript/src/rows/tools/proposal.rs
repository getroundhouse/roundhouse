//! A call that proposed a decision too large for a question, or revised one, drawn from the proposal's record, which
//! names the call that wrote each revision: `Proposed`, or `Revised`, its title in a chip, which revision it wrote once
//! there are several, what it asks the person to choose and that it waits for them, or what was said to that
//! revision, and a button that opens it in the proposal tab. A call that withdrew one is drawn from the record too:
//! `Withdrew`, the title, and why. What the agent wrote is read in the tab, so the line
//! doesn't open. A call whose proposal hasn't come, or which the hub refused, is drawn as any other tool's.
//!
//! The prompt the hub sent the agent to say what was chosen is drawn as a line like it, `Answered`, or `Replied` for
//! another round, where the agent took it, rather than as something the person said, since they answered in the tab,
//! with the revision it answered once there are several. It's sent at once, so while the agent works it waits for the
//! agent's next chance, faint until then, as the person's own prompt does, and one the agent never got says so.

use client_core::calls::count;
use client_core::session_log::PromptState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, div,
};
use look::Typeset as _;
use protocol::model::{
    Proposal, ProposalId, ResponseSummary, RevisionNumber, RevisionStanding, RevisionSummary, Seq, chosen_in_words,
};

use super::super::around::Answers;
use super::super::{Shared, row};
use super::Toggle;
use super::line::{Line, Part};
use crate::transcript::{Did, ProposalCall};
use crate::{PROPOSAL_ICON, TranscriptEvent};

/// The line of a call that did something to a proposal, as `call` says: wrote a revision, with what became of it, or
/// withdrew it, with why. It doesn't open, and has Open at its end.
pub(super) fn draw(call: &ProposalCall, toggle: &Toggle, cx: &App) -> AnyElement {
    let ProposalCall { proposal, did } = call;
    let title = proposal.current.proposed.title.trim().to_string();
    // Each line is its own element, the first revision's as it was before there were revisions.
    let (verb, said, id, button) = match *did {
        Did::Wrote(revision) => {
            let verb = if revision == RevisionNumber::FIRST { "Proposed" } else { "Revised" };
            let id = match revision.0 {
                1 => format!("proposed/{}", proposal.id),
                number => format!("revised/{}/{number}", proposal.id),
            };
            (verb, standing(proposal, revision), id, opens(&proposal.id, revision))
        }
        Did::Withdrew => {
            let why = proposal.withdrawn.as_ref().map(|withdrawal| withdrawal.why.trim().to_string());
            let button = ElementId::Name(format!("open-withdrawn/{}", proposal.id).into());
            ("Withdrew", why.unwrap_or_default(), format!("withdrew/{}", proposal.id), button)
        }
    };
    let label = format!("{verb} {title}: {said}");
    let line = Line::new(PROPOSAL_ICON, verb)
        .part(Part::Chip(look::chip(look::ChipKind::Proposal, look::ChipSize::Regular, title)))
        .part(Part::Note(said.into()))
        .action(open(&proposal.id, button, toggle.shared));
    // It doesn't open, since what the agent wrote is read in the tab, which Open brings up. A call that failed made no
    // proposal, so there's nothing under it to say either.
    line.still(ElementId::Name(id.into()), label, &toggle.shared.line_text, cx)
}

/// What an answer the agent never got says under its line, and where the agent can still read it.
const NEVER_GOT: &str = "The agent stopped before it read this, so it never got your answer. Ask it to read the \
    proposal, which keeps it.";

/// The prompt at `seq`, which told the agent what was chosen in answer to the proposal it names: its title and what
/// was chosen, once its record has come, and until then that it answered a proposal. Faint until the agent has it, in
/// `state`, and saying so when it never got it.
pub(in super::super) fn answered(
    seq: Seq,
    answers: &Answers,
    state: PromptState,
    shared: &Shared,
    cx: &App,
) -> AnyElement {
    let ResponseLine { verb, title, said } = response_line(answers.proposal.as_deref(), answers.revision);
    let label = match &title {
        Some(title) => format!("{verb} {title}: {said}"),
        None => format!("{verb} {said}"),
    };
    let chip = title.map(|title| Part::Chip(look::chip(look::ChipKind::Proposal, look::ChipSize::Regular, title)));
    let line = Line::new(PROPOSAL_ICON, verb).part(chip).part(Part::Note(said.into()));
    // Its own, since the proposal's line has one too.
    let line = line.action(open(&answers.id, ElementId::Name(format!("open-response/{seq}").into()), shared));
    let id = ElementId::Name(format!("answered-proposal/{seq}").into());
    let line =
        div().when(state != PromptState::Taken, |d| d.opacity(0.6)).child(line.still(id, label, &shared.line_text, cx));
    let never_got = (state == PromptState::Dropped).then(|| {
        div()
            .id(ElementId::Name(format!("answer-never-got/{seq}").into()))
            .typeset(look::Type::Detail)
            .text_color(look::colors(cx).text_faint)
            .child(NEVER_GOT)
            .test_support()
    });
    // On its own row, spaced as a row of calls is, so it reads as one of them.
    row().pb(look::Space::Small).child(line).children(never_got).into_any_element()
}

/// What became of the revision `revision` of `proposal`, after which it is once there are several: for the newest,
/// that the agent withdrew it, as the proposal's standing says, as the tab does, and otherwise what was said to it,
/// or, for the newest, what it asks the person to choose and that it waits for them, or, for an earlier one, that the
/// next replaced it before they answered.
fn standing(proposal: &Proposal, revision: RevisionNumber) -> String {
    let newest = revision == proposal.current.number;
    let withdrawn = newest && matches!(proposal.standing(), RevisionStanding::Withdrawn(_));
    let said = match responded(proposal, revision) {
        _ if withdrawn => format!("{} · withdrawn", asks(proposal)),
        Some(said) => said,
        None if newest => format!("{} · waiting for you", asks(proposal)),
        None => format!("replaced by revision {}", revision.next()),
    };
    if proposal.revised() { format!("Revision {revision} · {said}") } else { said }
}

/// What a line telling of the person's response to a revision of a proposal says, as the chat's line says it and the
/// queue above the composer does while the prompt that tells the agent waits there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseLine {
    /// `Replied` for another round, and `Answered` otherwise.
    pub verb: &'static str,
    /// The proposal's title, for its chip, once its record has come.
    pub title: Option<String>,
    /// What was said, in which revision once there are several, or only that it answers a proposal, until its record
    /// has come.
    pub said: String,
}

/// What a line telling of the response to the revision `revision` of `proposal` says, as [`ResponseLine`] says.
pub fn response_line(proposal: Option<&Proposal>, revision: RevisionNumber) -> ResponseLine {
    let Some(proposal) = proposal else {
        return ResponseLine { verb: "Answered", title: None, said: "a proposal".to_string() };
    };
    let summary = summary(proposal, revision);
    let replied = matches!(summary.and_then(|summary| summary.response), Some(ResponseSummary::AnotherRound { .. }));
    let said = responded(proposal, revision).unwrap_or_default();
    let said = if proposal.revised() { format!("{said}, in revision {revision}") } else { said };
    let verb = if replied { "Replied" } else { "Answered" };
    ResponseLine { verb, title: Some(proposal.current.proposed.title.trim().to_string()), said }
}

/// How the person responded to the revision `revision` of `proposal`, once they have, as its lines say it: what was
/// chosen, and whether a note went with it, or that they asked for another round.
fn responded(proposal: &Proposal, revision: RevisionNumber) -> Option<String> {
    Some(match summary(proposal, revision)?.response? {
        ResponseSummary::Chose { chosen, note, .. } => {
            let chosen = format!("Chose {}", chosen_in_words(&chosen));
            if note.is_empty() { chosen } else { format!("{chosen} · with a note") }
        }
        ResponseSummary::AnotherRound { .. } => "Another round, with a note".to_string(),
        ResponseSummary::Unknown => "Responded".to_string(),
    })
}

/// The revision `revision` of `proposal` in short: the newest from the record in full, and an earlier one from its
/// summary.
fn summary(proposal: &Proposal, revision: RevisionNumber) -> Option<RevisionSummary> {
    if revision == proposal.current.number {
        Some(proposal.current.summary())
    } else {
        proposal.revision(revision).cloned()
    }
}

/// What it asks the person to choose: how many options, for one question, or how many questions.
fn asks(proposal: &Proposal) -> String {
    match proposal.current.proposed.questions.as_slice() {
        [one] => count(one.options.len(), "option", "options"),
        questions => count(questions.len(), "question", "questions"),
    }
}

/// The Open on the line of the revision `revision` of `proposal`: by the proposal, and on a revision after the first, by
/// the revision too, since its proposal has a line for each.
fn opens(proposal: &ProposalId, revision: RevisionNumber) -> ElementId {
    let button = match revision {
        RevisionNumber::FIRST => format!("open-proposal/{proposal}"),
        revision => format!("open-proposal/{proposal}/{revision}"),
    };
    ElementId::Name(button.into())
}

/// The button `button` that opens `proposal` in the proposal tab. It says Open, and it's outlined, as a subagent's is.
fn open(proposal: &ProposalId, button: ElementId, shared: &Shared) -> AnyElement {
    let (this, id) = (shared.this.clone(), proposal.clone());
    look::button(button, look::ButtonKind::Outlined, look::ControlSize::Small, "Open")
        .icon(gpui_kit::assets::IconName::ArrowUpRight)
        .tooltip("Open in the proposal tab")
        .on_click(move |_, _, cx| {
            // The line under it would take the click too.
            cx.stop_propagation();
            let proposal = id.clone();
            let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::OpenProposal { proposal }));
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use protocol::model::{Proposal, Response, RevisionNumber, Round, Withdrawal};

    /// Withdrawn after the person asked for another round, as the agent may when the round shows it isn't needed: the
    /// proposal's line says withdrawn, as its tab does, rather than what was said to it.
    #[test]
    fn a_line_says_withdrawn_whatever_was_said_to_the_newest() {
        let mut proposal = protocol::fixtures::proposal("p");
        proposal.current.response = Some(Response::AnotherRound(Round { note: "Smaller".into(), at_ms: 0 }));
        proposal.revisions = vec![proposal.current.summary()];
        let asked = proposal.clone();
        let withdrawn =
            Proposal { withdrawn: Some(Withdrawal { why: "Decided".into(), call: None, at_ms: 0 }), ..proposal };

        let said = [&asked, &withdrawn].map(|proposal| super::standing(proposal, RevisionNumber::FIRST));

        assert_eq!(said, ["Another round, with a note", "2 options · withdrawn"]);
    }
}
