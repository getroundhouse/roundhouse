//! A call that proposed a decision too large for a question, drawn from the proposal's record, which names the call:
//! `Proposed`, its title in a chip, what it asks the person to choose and that it waits for them, or what they chose,
//! and a button that opens it in the proposal tab. What the agent wrote is read in the tab, so the line doesn't open. A
//! call whose proposal hasn't come, or which the hub refused, is drawn as any other tool's.
//!
//! The prompt the hub sent the agent to say what was chosen is drawn as a line like it, `Answered`, where the agent
//! took it, rather than as something the person said, since they answered in the tab. It's sent at once, so while the
//! agent works it waits for the agent's next chance, faint until then, as the person's own prompt does, and one the
//! agent never got says so.

use std::sync::Arc;

use client_core::calls::count;
use client_core::session_log::PromptState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, div,
};
use look::Typeset as _;
use protocol::model::{Proposal, ProposalId, RevisionStanding, Seq};

use super::super::around::Answers;
use super::super::{Shared, row};
use super::Toggle;
use super::line::{Line, Part};
use crate::{PROPOSAL_ICON, TranscriptEvent};

/// The line of the call that made `proposal`, which doesn't open, with Open at its end.
pub(super) fn draw(proposal: &Arc<Proposal>, toggle: &Toggle, cx: &App) -> AnyElement {
    let (title, standing) = (proposal.current.proposed.title.trim().to_string(), standing(proposal));
    let label = format!("Proposed {title}: {standing}");
    let line = Line::new(PROPOSAL_ICON, "Proposed")
        .part(Part::Chip(look::chip(look::ChipKind::Proposal, look::ChipSize::Regular, title)))
        .part(Part::Note(standing.into()))
        .action(open(&proposal.id, toggle.shared));
    // It doesn't open, since what the agent wrote is read in the tab, which Open brings up. A call that failed made no
    // proposal, so there's nothing under it to say either.
    line.still(ElementId::Name(format!("proposed/{}", proposal.id).into()), label, &toggle.shared.line_text, cx)
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
    let (title, said) = answered_parts(answers.proposal.as_deref());
    let label = match &title {
        Some(title) => format!("Answered {title}: {said}"),
        None => format!("Answered {said}"),
    };
    let chip = title.map(|title| Part::Chip(look::chip(look::ChipKind::Proposal, look::ChipSize::Regular, title)));
    let line = Line::new(PROPOSAL_ICON, "Answered").part(chip).part(Part::Note(said.into()));
    let line = line.action(open(&answers.id, shared));
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

/// Whether it waits for the person, with what it asks them to choose, or what they chose.
fn standing(proposal: &Proposal) -> String {
    responded(proposal).unwrap_or_else(|| format!("{} · waiting for you", asks(proposal)))
}

/// What a line telling of the answer to `proposal` says after `Answered`, as the chat's line says it and the queue above
/// the composer does while the prompt that tells the agent waits there: the proposal's title, for its chip, and what
/// was chosen, or only that it answers a proposal, until its record has come.
pub fn answered_parts(proposal: Option<&Proposal>) -> (Option<String>, String) {
    match proposal {
        Some(proposal) => {
            (Some(proposal.current.proposed.title.trim().to_string()), responded(proposal).unwrap_or_default())
        }
        None => (None, "a proposal".to_string()),
    }
}

/// How the person responded, once they have, as its lines say it: what was chosen, and whether a note went with it.
fn responded(proposal: &Proposal) -> Option<String> {
    match proposal.current.standing() {
        RevisionStanding::Waiting => None,
        RevisionStanding::Chose(choice) => {
            let chosen = format!("Chose {}", proposal.current.chosen_in_words().unwrap_or_default());
            Some(if choice.note.is_empty() { chosen } else { format!("{chosen} · with a note") })
        }
        RevisionStanding::Responded => Some("Responded".to_string()),
    }
}

/// What it asks the person to choose: how many options, for one question, or how many questions.
fn asks(proposal: &Proposal) -> String {
    match proposal.current.proposed.questions.as_slice() {
        [one] => count(one.options.len(), "option", "options"),
        questions => count(questions.len(), "question", "questions"),
    }
}

/// The button that opens the proposal in the proposal tab. It says Open, and it's outlined, as a subagent's is.
fn open(proposal: &ProposalId, shared: &Shared) -> AnyElement {
    let (this, id) = (shared.this.clone(), proposal.clone());
    let button = ElementId::Name(format!("open-proposal/{proposal}").into());
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
