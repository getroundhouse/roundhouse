//! A proposal in words, for the agent that wrote it to read back, such as after its conversation was compacted.

use protocol::model::{
    Proposal, ProposalQuestion, ProposalRevision, ResponseSummary, RevisionStanding, chosen_in_words,
};

/// The whole proposal, as its revision `revision` says it, or its newest: its title and ID, which revision it is once
/// there are several, and what changed in it, its context, then each question with its options and what was
/// recommended, and the person's answer or request for another round, or that it waits for them.
pub(super) fn describe(proposal: &Proposal, revision: Option<&ProposalRevision>) -> String {
    let current = revision.unwrap_or(&proposal.current);
    let proposed = &current.proposed;
    let mut parts = vec![format!("Proposal {}, \"{}\".", proposal.id, proposed.title)];
    if proposal.revised() {
        let newest = proposal.current.number;
        parts.push(match current.number {
            number if number == newest => format!("This is revision {number}, the newest."),
            number => format!("This is revision {number}. The newest is revision {newest}."),
        });
    }
    if !current.changed.trim().is_empty() {
        parts.push(format!("What changed since the revision before: {}", current.changed.trim()));
    }
    if !proposed.context.trim().is_empty() {
        parts.push(proposed.context.trim().to_string());
    }
    parts.extend(proposed.questions.iter().enumerate().map(|(at, question)| question_in_words(at + 1, question)));
    let newest = current.number == proposal.current.number;
    // Only the newest says it's withdrawn, as the proposal's standing has it.
    let standing = if newest { proposal.standing() } else { current.standing() };
    parts.push(answer_in_words(current, standing, newest));
    if newest {
        parts.extend(responded_before(proposal));
    }
    parts.join("\n\n")
}

/// What the person said to the newest of the revisions before `proposal`'s newest that they responded to, and how to
/// read it. Their prompt for it may reach the agent after it revised, naming no revision when there was only the one,
/// so whatever reads the newest is told of it.
pub(super) fn responded_before(proposal: &Proposal) -> Option<String> {
    let newest = proposal.current.number;
    let (before, response) = proposal
        .revisions
        .iter()
        .rev()
        .filter(|revision| revision.number < newest)
        .find_map(|revision| Some((revision.number, revision.response.as_ref()?)))?;
    let what = match response {
        ResponseSummary::Chose { chosen, .. } => format!("chose {}", chosen_in_words(chosen)),
        ResponseSummary::AnotherRound { .. } => "asked for another round".to_string(),
        ResponseSummary::Unknown => "responded".to_string(),
    };
    Some(format!(
        "The person {what}, in revision {before}, before you revised it. If you haven't had their prompt for it yet, \
         it's on its way, and get_proposal reads what they said, with their note, given revision {before}."
    ))
}

/// What the person said to `current`, as `standing` says, which, if it isn't the `newest`, was revised before they
/// answered when they said nothing, or that the agent withdrew it.
fn answer_in_words(current: &ProposalRevision, standing: RevisionStanding<'_>, newest: bool) -> String {
    let choice = match standing {
        RevisionStanding::Waiting if newest => return "The person hasn't answered yet.".to_string(),
        RevisionStanding::Waiting => return "You revised it before the person answered.".to_string(),
        RevisionStanding::Responded => {
            return "The person has responded, in a way this version of Roundhouse can't describe.".to_string();
        }
        RevisionStanding::AnotherRound(round) => {
            return format!(
                "The person asked for another round, without choosing, and wrote what to change:\n{}",
                round.note
            );
        }
        RevisionStanding::Withdrawn(withdrawal) => {
            return format!("You withdrew it before the person chose: {}", withdrawal.why);
        }
        RevisionStanding::Chose(choice) => choice,
    };
    let chosen = current.chosen_in_words().unwrap_or_default();
    if choice.note.is_empty() {
        return format!("The person chose {chosen}.");
    }
    format!("The person chose {chosen}, and wrote:\n{}", choice.note)
}

fn question_in_words(number: usize, question: &ProposalQuestion) -> String {
    let mut parts = vec![format!("Question {number}, {}: {}", question.header.trim(), question.text.trim())];
    if !question.details.trim().is_empty() {
        parts.push(question.details.trim().to_string());
    }
    for (letter, option) in question.places().filter_map(|place| question.named(place)) {
        let description = option.description.trim();
        let description = if description.is_empty() { String::new() } else { format!(": {description}") };
        let mut said = format!("{letter}. {}{description}", option.label.trim());
        if !option.body.trim().is_empty() {
            said = format!("{said}\n{}", option.body.trim());
        }
        parts.push(said);
    }
    if let Some((recommended, letter)) =
        question.recommended.as_ref().and_then(|r| Some((r, question.letter(r.option)?)))
    {
        let why =
            if recommended.why.trim().is_empty() { String::new() } else { format!(" {}", recommended.why.trim()) };
        parts.push(format!("You recommended {letter}.{why}"));
    }
    parts.join("\n\n")
}
