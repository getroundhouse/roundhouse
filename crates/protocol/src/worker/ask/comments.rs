//! What a worker asks its hub about the comments on the workspace's diff, for an agent to read.

use serde::{Deserialize, Serialize};

use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::DiffComment;

/// The comments left on the workspace's diff, as the hub keeps them. The one being written isn't among them, since
/// it isn't saved yet.
#[derive(Clone, Copy, Debug)]
pub struct ReadDiffComments;

impl Ask for ReadDiffComments {
    type Answer = DiffComments;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::DiffComments
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DiffComments {
    /// Oldest first.
    pub comments: Vec<DiffComment>,
}

impl Answer for DiffComments {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::DiffComments(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}
