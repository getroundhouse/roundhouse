//! The person's response to a revision of a proposal, which the hub keeps on it, and why an answer can't be given.

use serde::{Deserialize, Serialize};

use super::{OptionPlace, Proposal, ProposalRevision, Proposed, RevisionNumber, Withdrawal, chosen_in_words};
use crate::client::{ApiError, ErrorCode};

/// The most text an answer's note holds, which the agent reads with the proposal. *Unconfirmed.*
pub const MAX_NOTE_BYTES: usize = 16 * 1024;

/// How the person responded to a revision of a proposal: by choosing, or by asking for another round without
/// choosing, such as when none of the options will do. A newer hub's kind that this build doesn't know reads as
/// `Unknown`, which is a response, so the revision doesn't wait for the person, but says nothing of what it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Chose(Choice),
    AnotherRound(Round),
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

/// The person's request for another revision, without choosing, with the note that says what it should change.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Round {
    /// What the next revision should change, which the agent reads with the proposal rather than in the prompt. Never
    /// empty.
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
    /// Another round was asked for without a note, which is what tells the agent what to change.
    #[error("say in a note what the next revision should change")]
    NoNote,
    /// A kind of response this build doesn't know, which it can't check, so never gives.
    #[error("this version of Roundhouse can't give that kind of response")]
    Unknown,
    #[error("the proposal has been answered already")]
    Answered,
    /// The agent has written a newer revision than the one answered, which the person hasn't read.
    #[error("the agent has revised the proposal since you read it, as revision {newest}")]
    Revised { newest: RevisionNumber },
    /// The agent has withdrawn it, so nothing more can be said to it.
    #[error("the agent has withdrawn the proposal")]
    Withdrawn,
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
        check_note(note)
    }

    /// Whether `response` can be given to a revision proposing this: a choice that [`Proposed::check_answer`] takes, or
    /// another round with a note that fits. A kind this build doesn't know can't be checked, so isn't given.
    pub fn check_response(&self, response: &Response) -> Result<(), AnswerProblem> {
        match response {
            Response::Chose(choice) => self.check_answer(&choice.chosen, &choice.note),
            Response::AnotherRound(Round { note, .. }) if note.trim().is_empty() => Err(AnswerProblem::NoNote),
            Response::AnotherRound(Round { note, .. }) => check_note(note),
            Response::Unknown => Err(AnswerProblem::Unknown),
        }
    }
}

fn check_note(note: &str) -> Result<(), AnswerProblem> {
    if note.len() > MAX_NOTE_BYTES {
        return Err(AnswerProblem::LongNote(note.len()));
    }
    Ok(())
}

/// Where a revision stands with the person, which every reader asks, rather than whether it has a response or a
/// choice, which a response of a kind this build doesn't know would set apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionStanding<'a> {
    /// It waits for them.
    Waiting,
    /// They chose.
    Chose(&'a Choice),
    /// They asked for another round, so it waits for the agent's next revision.
    AnotherRound(&'a Round),
    /// They responded in a way only a newer hub knows.
    Responded,
    /// The agent withdrew the proposal before they chose, which only its newest revision says, as
    /// [`Proposal::standing`] has it.
    Withdrawn(&'a Withdrawal),
}

impl Proposal {
    /// Where its newest revision stands with the person: withdrawn, once the agent withdrew it before they chose, or
    /// as the revision says.
    pub fn standing(&self) -> RevisionStanding<'_> {
        match (&self.withdrawn, self.current.standing()) {
            (Some(withdrawal), RevisionStanding::Waiting | RevisionStanding::AnotherRound(_)) => {
                RevisionStanding::Withdrawn(withdrawal)
            }
            (_, standing) => standing,
        }
    }
}

impl ProposalRevision {
    /// Where it stands with the person, from what they said to it alone, so never withdrawn, which only
    /// [`Proposal::standing`] knows of.
    pub fn standing(&self) -> RevisionStanding<'_> {
        match &self.response {
            None => RevisionStanding::Waiting,
            Some(Response::Chose(choice)) => RevisionStanding::Chose(choice),
            Some(Response::AnotherRound(round)) => RevisionStanding::AnotherRound(round),
            Some(Response::Unknown) => RevisionStanding::Responded,
        }
    }

    /// What the person chose, if they responded by choosing.
    pub fn choice(&self) -> Option<&Choice> {
        self.response.as_ref()?.choice()
    }

    /// What was chosen, in words, for the chat's line and the prompt, as [`chosen_in_words`] says.
    pub fn chosen_in_words(&self) -> Option<String> {
        Some(chosen_in_words(&self.chosen()?))
    }
}

/// The hub refuses such an answer as an invalid request, or one to a proposal answered already, or revised since, as a
/// conflict, saying why.
impl From<AnswerProblem> for ApiError {
    fn from(problem: AnswerProblem) -> Self {
        let conflict =
            matches!(problem, AnswerProblem::Answered | AnswerProblem::Revised { .. } | AnswerProblem::Withdrawn);
        let code = if conflict { ErrorCode::Conflict } else { ErrorCode::InvalidRequest };
        ApiError::new(code, problem.to_string())
    }
}
