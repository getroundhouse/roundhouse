//! The person's response to a revision of a proposal, which the hub keeps on it, and why an answer can't be given.

use serde::{Deserialize, Serialize};

use super::{OptionPlace, ProposalRevision, Proposed, RevisionNumber};
use crate::client::{ApiError, ErrorCode};

/// The most text an answer's note holds, which the agent reads with the proposal. *Unconfirmed.*
pub const MAX_NOTE_BYTES: usize = 16 * 1024;

/// How the person responded to a revision of a proposal, of which choosing is the only kind built. *Not built:*
/// replying without choosing, such as that none of the options will do, which asks the agent for another revision,
/// and would be another kind here. A newer hub's kind that this build doesn't know reads as `Unknown`, which is a
/// response, so the revision doesn't wait for the person, but says nothing of what it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Chose(Choice),
    #[serde(other)]
    Unknown,
}

impl Response {
    /// What was chosen, if the person chose.
    pub fn choice(&self) -> Option<&Choice> {
        if let Response::Chose(choice) = self { Some(choice) } else { None }
    }
}

/// What the person chose, for each question, and wrote with it: their answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// One option for each question, in the questions' order.
    pub chosen: Vec<OptionPlace>,
    /// What they wrote with it, which the agent reads with the proposal rather than in the prompt. Empty when they
    /// wrote nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub at_ms: i64,
}

/// Why an answer can't be given. The message is written for the person, whose client says it.
#[derive(thiserror::Error, Clone, Debug, PartialEq, Eq)]
pub enum AnswerProblem {
    #[error("choose one option for each of the proposal's {questions} questions")]
    Unchosen { questions: usize },
    #[error("question {question} has no such option")]
    NoSuchOption { question: usize },
    #[error("a note holds at most {MAX_NOTE_BYTES} bytes, and this holds {0}")]
    LongNote(usize),
    #[error("the proposal has been answered already")]
    Answered,
    /// The agent has written a newer revision than the one answered, which the person hasn't read.
    #[error("the agent has revised the proposal since you read it, as revision {newest}")]
    Revised { newest: RevisionNumber },
}

impl Proposed {
    /// Whether `chosen` is one of each question's options, one for each question, and `note` fits.
    pub fn check_answer(&self, chosen: &[OptionPlace], note: &str) -> Result<(), AnswerProblem> {
        if chosen.len() != self.questions.len() {
            return Err(AnswerProblem::Unchosen { questions: self.questions.len() });
        }
        for (at, (question, place)) in self.questions.iter().zip(chosen).enumerate() {
            if question.named(*place).is_none() {
                return Err(AnswerProblem::NoSuchOption { question: at + 1 });
            }
        }
        if note.len() > MAX_NOTE_BYTES {
            return Err(AnswerProblem::LongNote(note.len()));
        }
        Ok(())
    }
}

/// Where a revision stands with the person, which every reader asks, rather than whether it has a response or a
/// choice, which a response of a kind this build doesn't know would set apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionStanding<'a> {
    /// It waits for them.
    Waiting,
    /// They chose.
    Chose(&'a Choice),
    /// They responded in a way only a newer hub knows.
    Responded,
}

impl ProposalRevision {
    pub fn standing(&self) -> RevisionStanding<'_> {
        match &self.response {
            None => RevisionStanding::Waiting,
            Some(Response::Chose(choice)) => RevisionStanding::Chose(choice),
            Some(Response::Unknown) => RevisionStanding::Responded,
        }
    }

    /// What the person chose, if they responded by choosing.
    pub fn choice(&self) -> Option<&Choice> {
        self.response.as_ref()?.choice()
    }

    /// What was chosen, in words, for the chat's line and the prompt: each option's letter and label, by its
    /// question's header when there are several.
    pub fn chosen_in_words(&self) -> Option<String> {
        let answered = self.choice()?;
        let named = |(question, place): (&super::ProposalQuestion, &OptionPlace)| match question.named(*place) {
            Some((letter, option)) => format!("{letter}, {}", option.label.trim()),
            None => String::new(),
        };
        let pairs = self.proposed.questions.iter().zip(&answered.chosen);
        Some(match self.proposed.questions.as_slice() {
            [_] => pairs.map(named).collect::<String>(),
            _ => pairs.map(|pair| format!("{}: {}", pair.0.header.trim(), named(pair))).collect::<Vec<_>>().join("; "),
        })
    }
}

/// The hub refuses such an answer as an invalid request, or one to a proposal answered already, or revised since, as a
/// conflict, saying why.
impl From<AnswerProblem> for ApiError {
    fn from(problem: AnswerProblem) -> Self {
        let conflict = matches!(problem, AnswerProblem::Answered | AnswerProblem::Revised { .. });
        let code = if conflict { ErrorCode::Conflict } else { ErrorCode::InvalidRequest };
        ApiError::new(code, problem.to_string())
    }
}
