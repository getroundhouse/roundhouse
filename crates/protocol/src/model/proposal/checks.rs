use super::{OptionLetter, ProposalQuestion, Proposed};
use crate::client::{ApiError, ErrorCode};

/// The most questions one proposal asks, as the question tool asks at most four. A project with more splits into
/// proposals, by phase. *Unconfirmed.*
pub const MAX_QUESTIONS: usize = 4;

/// The fewest options a question offers, since one option is nothing to decide.
pub const MIN_OPTIONS: usize = 2;

/// The most options a question offers, so they can be compared side by side. *Unconfirmed.*
pub const MAX_OPTIONS: usize = 4;

/// The most characters in a question's header, which the outline and the answer bar name it by, at a width that holds
/// them. *Unconfirmed.*
pub const MAX_HEADER_CHARS: usize = 30;

/// The most text a proposal holds in all, since every client that shows its chat is sent it. A large picture goes in
/// as a file, as a prompt's does. *Unconfirmed.*
pub const MAX_PROPOSAL_BYTES: usize = 128 * 1024;

/// Why a proposal can't be made as it's written. The message is written for the agent, which reads it as the tool's
/// result and writes it again.
#[derive(thiserror::Error, Clone, Debug, PartialEq, Eq)]
pub enum ProposalProblem {
    #[error("a proposal needs a title")]
    NoTitle,
    #[error("a proposal has from 1 to {MAX_QUESTIONS} questions, and this has {0}")]
    Questions(usize),
    #[error("question {question} needs a header and its question")]
    Unasked { question: usize },
    #[error("question {question}'s header has {chars} characters, and a header has at most {MAX_HEADER_CHARS}")]
    LongHeader { question: usize, chars: usize },
    #[error("question {question} has {options} options, and a question has from {MIN_OPTIONS} to {MAX_OPTIONS}")]
    Options { question: usize, options: usize },
    #[error("option {letter} of question {question} needs a label")]
    Unlabelled { question: usize, letter: OptionLetter },
    #[error("question {question} recommends an option it doesn't have")]
    NoSuchOption { question: usize },
    #[error("a proposal holds at most {MAX_PROPOSAL_BYTES} bytes of text, and this holds {0}")]
    TooLong(usize),
}

impl Proposed {
    /// Whether it can be made: it has a title, from one to [`MAX_QUESTIONS`] questions, each with a header that fits,
    /// its question, from [`MIN_OPTIONS`] to [`MAX_OPTIONS`] options with a label each, and a recommendation of one
    /// of them if any, and it holds no more than [`MAX_PROPOSAL_BYTES`] of text. Questions are counted from 1, as the
    /// agent reads them.
    pub fn check(&self) -> Result<(), ProposalProblem> {
        if self.title.trim().is_empty() {
            return Err(ProposalProblem::NoTitle);
        }
        if !(1..=MAX_QUESTIONS).contains(&self.questions.len()) {
            return Err(ProposalProblem::Questions(self.questions.len()));
        }
        for (at, question) in self.questions.iter().enumerate() {
            question.check(at + 1)?;
        }
        let bytes = self.bytes();
        if bytes > MAX_PROPOSAL_BYTES {
            return Err(ProposalProblem::TooLong(bytes));
        }
        Ok(())
    }

    /// How much text it holds, in all.
    fn bytes(&self) -> usize {
        let question = |question: &ProposalQuestion| {
            let options = question.options.iter().map(|o| o.label.len() + o.description.len() + o.body.len());
            let why = question.recommended.as_ref().map_or(0, |r| r.why.len());
            question.header.len() + question.text.len() + question.details.len() + why + options.sum::<usize>()
        };
        self.title.len() + self.context.len() + self.questions.iter().map(question).sum::<usize>()
    }
}

impl ProposalQuestion {
    fn check(&self, question: usize) -> Result<(), ProposalProblem> {
        if self.header.trim().is_empty() || self.text.trim().is_empty() {
            return Err(ProposalProblem::Unasked { question });
        }
        let chars = self.header.trim().chars().count();
        if chars > MAX_HEADER_CHARS {
            return Err(ProposalProblem::LongHeader { question, chars });
        }
        let options = self.options.len();
        if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&options) {
            return Err(ProposalProblem::Options { question, options });
        }
        let unlabelled = self.places().filter_map(|place| self.named(place)).find(|(_, o)| o.label.trim().is_empty());
        if let Some((letter, _)) = unlabelled {
            return Err(ProposalProblem::Unlabelled { question, letter });
        }
        match &self.recommended {
            Some(recommended) if self.named(recommended.option).is_none() => {
                Err(ProposalProblem::NoSuchOption { question })
            }
            Some(_) | None => Ok(()),
        }
    }
}

/// The hub refuses such a proposal as an invalid request, saying why, for the agent to write it again.
impl From<ProposalProblem> for ApiError {
    fn from(problem: ProposalProblem) -> Self {
        ApiError::new(ErrorCode::InvalidRequest, problem.to_string())
    }
}
