//! What a person sends an agent, and what it has to be to go, sent or queued, so the hub and every client refuse the
//! same ones and say the same about them.

use serde::{Deserialize, Serialize};

use crate::attachment::MAX_PER_PROMPT;
use crate::client::{ApiError, ErrorCode};

/// A prompt: what it says, and the files it carries, in order. A file passes through stages on its way to the agent,
/// and `F` is the stage this prompt's are at: the bytes a client holds, the ID it added each under, the hub's record of
/// each, or where the worker put each in the worktree. A prompt with any file can say nothing.
///
/// Requests, records and events that carry a prompt hold one of these, flattened, so its fields are theirs on the wire,
/// as they were before it had a type. Its files are `attachments` there. It's read from the wire and from stored rows
/// whatever they hold, so nothing here is checked as it's made, and [`Prompt::check`] says whether one can go.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Prompt<F> {
    pub text: String,
    #[serde(rename = "attachments", default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<F>,
}

impl<F> Prompt<F> {
    /// A prompt that only says `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), files: Vec::new() }
    }

    pub fn carrying(text: impl Into<String>, files: Vec<F>) -> Self {
        Self { text: text.into(), files }
    }

    /// Whether it can go.
    pub fn check(&self) -> Result<(), PromptProblem> {
        if !self.says_or_carries() {
            return Err(PromptProblem::Empty);
        }
        if self.files.len() > MAX_PER_PROMPT {
            return Err(PromptProblem::TooManyFiles);
        }
        Ok(())
    }

    /// Whether it has anything to send: something said, apart from whitespace, or a file.
    pub fn says_or_carries(&self) -> bool {
        says_or_carries(&self.text, self.files.len())
    }

    /// Whether it says anything, apart from whitespace. One that doesn't gives an agent its files alone.
    pub fn says_anything(&self) -> bool {
        !self.text.trim().is_empty()
    }

    /// The same words, carrying `files` in place of these, such as the hub's record of each.
    pub fn with_files<G>(self, files: Vec<G>) -> Prompt<G> {
        Prompt { text: self.text, files }
    }
}

/// Whether what's typed, with `files` files, has anything to send, as [`Prompt::says_or_carries`] says, for what isn't
/// a prompt yet, such as what's in a composer, without copying it into one.
pub fn says_or_carries(text: &str, files: usize) -> bool {
    !text.trim().is_empty() || files > 0
}

/// Why a prompt can't go as it is. The message is written for the person who wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptProblem {
    /// It says nothing, apart from whitespace, and carries no file.
    Empty,
    /// It carries more than [`MAX_PER_PROMPT`] files.
    TooManyFiles,
}

impl std::fmt::Display for PromptProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptProblem::Empty => f.write_str("a prompt can't be empty"),
            PromptProblem::TooManyFiles => write!(f, "a prompt can carry at most {MAX_PER_PROMPT} files"),
        }
    }
}

impl std::error::Error for PromptProblem {}

/// The hub refuses such a prompt as an invalid request, saying why.
impl From<PromptProblem> for ApiError {
    fn from(problem: PromptProblem) -> Self {
        ApiError::new(ErrorCode::InvalidRequest, problem.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carrying(text: &str, files: usize) -> Prompt<u8> {
        Prompt::carrying(text, vec![0; files])
    }

    #[test]
    fn a_prompt_says_something_or_carries_something_and_no_more_than_it_can() {
        assert_eq!(carrying("look", 0).check(), Ok(()));
        assert_eq!(carrying(" \n", 1).check(), Ok(()), "files alone are enough");
        assert_eq!(carrying(" \n", 0).check(), Err(PromptProblem::Empty));
        assert_eq!(carrying("look", MAX_PER_PROMPT).check(), Ok(()));
        assert_eq!(carrying("look", MAX_PER_PROMPT + 1).check(), Err(PromptProblem::TooManyFiles));
    }

    #[test]
    fn what_s_typed_has_something_to_send_as_a_prompt_of_it_would() {
        for (text, files) in [("look", 0), (" \n", 1), (" \n", 0), ("", 0)] {
            assert_eq!(says_or_carries(text, files), carrying(text, files).says_or_carries(), "{text:?} and {files}");
        }
    }
}
