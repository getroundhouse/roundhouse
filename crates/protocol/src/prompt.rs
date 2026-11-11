//! What a prompt has to be to go, sent or queued, so the hub and every client refuse the same ones and say the same
//! about them.

use crate::attachment::MAX_PER_PROMPT;
use crate::client::{ApiError, ErrorCode};

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

/// Whether a prompt that says `text` and carries `files` files can go.
pub fn check_prompt(text: &str, files: usize) -> Result<(), PromptProblem> {
    if !says_or_carries(text, files) {
        return Err(PromptProblem::Empty);
    }
    if files > MAX_PER_PROMPT {
        return Err(PromptProblem::TooManyFiles);
    }
    Ok(())
}

/// Whether a prompt has anything to send: something said, apart from whitespace, or a file.
pub fn says_or_carries(text: &str, files: usize) -> bool {
    !text.trim().is_empty() || files > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_says_something_or_carries_something_and_no_more_than_it_can() {
        assert_eq!(check_prompt("look", 0), Ok(()));
        assert_eq!(check_prompt(" \n", 1), Ok(()), "files alone are enough");
        assert_eq!(check_prompt(" \n", 0), Err(PromptProblem::Empty));
        assert_eq!(check_prompt("look", MAX_PER_PROMPT), Ok(()));
        assert_eq!(check_prompt("look", MAX_PER_PROMPT + 1), Err(PromptProblem::TooManyFiles));
    }
}
