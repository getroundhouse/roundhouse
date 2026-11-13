//! Valid GitHub PR numbers and a nonempty, unambiguous selection.

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SelectionError {
    #[error("a pull request number must be between 1 and 2147483647")]
    Number,
    #[error("give at least one pull request number")]
    Empty,
    #[error("a pull request number was given more than once")]
    Duplicate,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "u64")]
pub struct PullRequestNumber(u64);

impl PullRequestNumber {
    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for PullRequestNumber {
    type Error = SelectionError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 || value > u64::try_from(i32::MAX).map_err(|_| SelectionError::Number)? {
            return Err(SelectionError::Number);
        }
        Ok(Self(value))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(try_from = "Vec<PullRequestNumber>")]
pub struct PullRequestSelection(Vec<PullRequestNumber>);

impl PullRequestSelection {
    pub fn numbers(&self) -> &[PullRequestNumber] {
        &self.0
    }
}

impl TryFrom<Vec<PullRequestNumber>> for PullRequestSelection {
    type Error = SelectionError;

    fn try_from(mut numbers: Vec<PullRequestNumber>) -> Result<Self, Self::Error> {
        if numbers.is_empty() {
            return Err(SelectionError::Empty);
        }
        numbers.sort_unstable();
        if numbers.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SelectionError::Duplicate);
        }
        Ok(Self(numbers))
    }
}
