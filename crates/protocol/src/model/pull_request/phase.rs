use super::facets::{PullRequest, Summary};
use super::status::{MergeState, PrState, QueueState};

/// What a pull request is, whatever's in the way of merging it, which its colour says wherever it's shown, as GitHub
/// colours it: a draft is a draft whether or not it conflicts, and one in the merge queue is queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Open,
    Draft,
    Queued(QueueState),
    Merged,
    Closed,
    /// GitHub said a state this version doesn't know.
    Unknown,
}

impl Summary {
    /// What it is. Done comes first, then the merge queue, which merges it whatever else is true, then whether it's a
    /// draft.
    pub fn phase(&self) -> Phase {
        match self.state {
            PrState::Merged => Phase::Merged,
            PrState::Closed => Phase::Closed,
            PrState::Unknown => Phase::Unknown,
            PrState::Open => match self.queue {
                Some(queue) => Phase::Queued(queue),
                None if self.is_draft || self.merge_state == MergeState::Draft => Phase::Draft,
                None => Phase::Open,
            },
        }
    }
}

impl PullRequest {
    /// What it is, as its summary says.
    pub fn phase(&self) -> Phase {
        self.said().phase()
    }
}
