//! A proposal in words, for the agent that wrote it to read back, such as after its conversation was compacted.

use protocol::model::{OptionPlace, Proposal, ProposalQuestion, RevisionStanding};

/// The whole proposal: its title and ID, its context, then each question with its options and what was recommended,
/// and the person's answer, or that it waits for one.
pub(super) fn describe(proposal: &Proposal) -> String {
    let proposed = &proposal.current.proposed;
    let mut parts = vec![format!("Proposal {}, \"{}\".", proposal.id, proposed.title)];
    if !proposed.context.trim().is_empty() {
        parts.push(proposed.context.trim().to_string());
    }
    parts.extend(proposed.questions.iter().enumerate().map(|(at, question)| question_in_words(at + 1, question)));
    parts.push(answer_in_words(proposal));
    parts.join("\n\n")
}

fn answer_in_words(proposal: &Proposal) -> String {
    let current = &proposal.current;
    let choice = match current.standing() {
        RevisionStanding::Waiting => return "The person hasn't answered yet.".to_string(),
        RevisionStanding::Responded => {
            return "The person has responded, in a way this version of Roundhouse can't describe.".to_string();
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
    for (option, place) in question.options.iter().zip((0..).map(OptionPlace)) {
        let description = option.description.trim();
        let description = if description.is_empty() { String::new() } else { format!(": {description}") };
        let mut said = format!("{}. {}{description}", place.letter(), option.label.trim());
        if !option.body.trim().is_empty() {
            said = format!("{said}\n{}", option.body.trim());
        }
        parts.push(said);
    }
    if let Some(recommended) = &question.recommended {
        let why =
            if recommended.why.trim().is_empty() { String::new() } else { format!(" {}", recommended.why.trim()) };
        parts.push(format!("You recommended {}.{why}", recommended.option.letter()));
    }
    parts.join("\n\n")
}
