//! What a proposal's record says of each of its revisions besides the newest, which it carries in full: enough for a
//! chat to draw each revision's line, and for the timeline of revisions, without reading each in full.

use serde::{Deserialize, Serialize};

use super::{Choice, OptionLetter, ProposalRevision, Proposed, Response, RevisionNumber};
use crate::model::ToolUseId;

/// The most of a note a summary carries, in characters: the start a timeline shows of it. `get_proposal` and
/// [`ProposalRevisionsRead`](crate::client::proposals::ProposalRevisionsRead) read it whole. *Unconfirmed.*
pub const NOTE_START_CHARS: usize = 200;

/// One revision of a proposal, in short: which it is, the call that wrote it, when, and what was said to it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RevisionSummary {
    pub number: RevisionNumber,
    /// As [`ProposalRevision::call`] says, which the chat draws the revision's line for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    pub made_at_ms: i64,
    /// The person's response, in short, once they've given one. `None` while it waits for them, and for a revision
    /// that was revised before they responded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<ResponseSummary>,
}

/// A response to a revision, in short. A newer hub's kind this build doesn't know reads as `Unknown`, as
/// [`Response`]'s does.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseSummary {
    Chose {
        /// One for each question, in the questions' order.
        chosen: Vec<ChosenOption>,
        /// The start of the note, at most [`NOTE_START_CHARS`] of it, or empty when the person wrote none.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        note: String,
        at_ms: i64,
    },
    /// The person asked for another round.
    AnotherRound {
        /// The start of the note, at most [`NOTE_START_CHARS`] of it.
        note: String,
        at_ms: i64,
    },
    #[serde(other)]
    Unknown,
}

/// An option the person chose, as they read it: by its question's header, its letter and its label.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChosenOption {
    pub header: String,
    pub letter: OptionLetter,
    pub label: String,
}

impl ProposalRevision {
    /// This revision in short, as its proposal's record carries it.
    pub fn summary(&self) -> RevisionSummary {
        RevisionSummary {
            number: self.number,
            call: self.call.clone(),
            made_at_ms: self.made_at_ms,
            response: self.response.as_ref().map(|response| response.summary(self.chosen().unwrap_or_default())),
        }
    }

    /// What the person chose, as they read it, if they chose.
    pub fn chosen(&self) -> Option<Vec<ChosenOption>> {
        Some(self.proposed.chosen(self.choice()?))
    }
}

impl Response {
    /// This response in short, with what was `chosen`, as the person read it, for a choice, which only a choice uses.
    /// So a summary is made without what the agent wrote, the largest part of a revision, unless the person chose.
    pub fn summary(&self, chosen: Vec<ChosenOption>) -> ResponseSummary {
        match self {
            Response::Chose(choice) => {
                ResponseSummary::Chose { chosen, note: note_start(&choice.note), at_ms: choice.at_ms }
            }
            Response::AnotherRound(round) => {
                ResponseSummary::AnotherRound { note: note_start(&round.note), at_ms: round.at_ms }
            }
            Response::Unknown => ResponseSummary::Unknown,
        }
    }
}

impl Proposed {
    /// What `choice` chose here, as the person read it: each option's question's header, letter and label.
    pub fn chosen(&self, choice: &Choice) -> Vec<ChosenOption> {
        let chosen = self.questions.iter().zip(&choice.chosen).filter_map(|(question, place)| {
            let (letter, option) = question.named(*place)?;
            Some(ChosenOption { header: question.header.trim().into(), letter, label: option.label.trim().into() })
        });
        chosen.collect()
    }
}

/// The start of `note` a summary carries.
fn note_start(note: &str) -> String {
    note.chars().take(NOTE_START_CHARS).collect()
}

/// What was chosen, in words, for the chat's line and the prompt: each option's letter and label, by its question's
/// header when there are several.
pub fn chosen_in_words(chosen: &[ChosenOption]) -> String {
    let named = |chosen: &ChosenOption| format!("{}, {}", chosen.letter, chosen.label);
    match chosen {
        [only] => named(only),
        _ => chosen.iter().map(|chosen| format!("{}: {}", chosen.header, named(chosen))).collect::<Vec<_>>().join("; "),
    }
}
