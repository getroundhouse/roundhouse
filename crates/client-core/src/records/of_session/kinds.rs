use protocol::model::{MessageId, Proposal, ProposalId, ReplyHeard, Rev, SessionId, Subagent, SubagentId};
use std::cmp::Ordering;
use std::fmt::Debug;
use std::hash::Hash;

/// A kind of record a session has many of, which a client keeps for the sessions a view shows.
pub(crate) trait OfSession: PartialEq + Debug {
    type Id: Clone + Eq + Hash + Ord + Debug;

    fn session(&self) -> &SessionId;
    /// Which of its session's it is.
    fn id(&self) -> Self::Id;
    fn rev(&self) -> Rev;
    /// Whether it's read before or after another of its session's.
    fn order(&self, other: &Self) -> Ordering;
}

impl OfSession for Subagent {
    type Id = SubagentId;

    fn session(&self) -> &SessionId {
        &self.session_id
    }

    fn id(&self) -> SubagentId {
        Subagent::id(self)
    }

    fn rev(&self) -> Rev {
        self.rev
    }

    /// In the order they started, or last started again.
    fn order(&self, other: &Self) -> Ordering {
        (self.started_at_ms, &self.call).cmp(&(other.started_at_ms, &other.call))
    }
}

impl OfSession for Proposal {
    type Id = ProposalId;

    fn session(&self) -> &SessionId {
        &self.session_id
    }

    fn id(&self) -> ProposalId {
        self.id.clone()
    }

    fn rev(&self) -> Rev {
        self.rev
    }

    /// In the order they were made.
    fn order(&self, other: &Self) -> Ordering {
        (self.created_at_ms, &self.id).cmp(&(other.created_at_ms, &other.id))
    }
}

impl OfSession for ReplyHeard {
    type Id = MessageId;

    fn session(&self) -> &SessionId {
        &self.session_id
    }

    fn id(&self) -> MessageId {
        self.message.clone()
    }

    fn rev(&self) -> Rev {
        self.rev
    }

    /// By reply, since each is read by its reply rather than in a list.
    fn order(&self, other: &Self) -> Ordering {
        self.message.cmp(&other.message)
    }
}
