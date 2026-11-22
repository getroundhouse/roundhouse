//! Remote pull requests watched by number, independently of a worktree's checked-out branch.

mod selection;
pub use selection::{PullRequestNumber, PullRequestSelection, SelectionError};

mod records;
pub use records::{
    GitHubRepository, PullRequestWaitResult, PullRequestWatch, PullRequestsSnapshot, WaitBaseline, WaitOutcome,
};
