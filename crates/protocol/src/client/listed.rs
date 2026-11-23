//! Where what a [`Subscribe`](super::subscriptions::Subscribe) subscription follows stands, as it's sent first.

use serde::{Deserialize, Serialize};

use crate::model::{Proposal, Records, ReplyHeard, Rev, SessionId, Subagent};

/// Every record, and every observation, of one level, read at one moment, as `HubFrame::Listed` carries it. A
/// subscription is sent one as it starts, and again whenever it has fallen behind. Each copy in it replaces the
/// client's, unless the client's is newer. One of the level the client holds that's missing from it, and isn't newer
/// than its revision, is gone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum Listed {
    /// Every record but those of one session, with the observations sent at the hub's level, as
    /// [`RecordsList`](super::records::RecordsList) returns them. Boxed, since it's the largest by far.
    Hub(Box<Records>),
    /// The records of one session.
    Session(SessionRecords),
    /// Of a level from a newer hub, which this client skips.
    #[serde(other)]
    Unknown,
}

/// The records of one session, read in one transaction: what `SessionSubagents`, `SessionProposals` and
/// `SessionRepliesHeard` each return, together.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionRecords {
    pub session_id: SessionId,
    /// Oldest first.
    pub subagents: Vec<Subagent>,
    /// Oldest first.
    pub proposals: Vec<Proposal>,
    /// In no order, since each is read by its reply.
    pub replies_heard: Vec<ReplyHeard>,
    /// The hub's newest revision when they were read.
    pub rev: Rev,
}
