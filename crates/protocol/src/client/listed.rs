//! Where what a [`Subscribe`](super::subscriptions::Subscribe) subscription follows stands, as it's sent first.

use serde::{Deserialize, Serialize};

use crate::model::{Observation, Proposal, Records, ReferenceAsk, ReplyHeard, Rev, SessionId, Subagent, WorkspaceId};

/// Every record, and every observation, of one level, read at one moment, as `HubFrame::Listed` carries it. A
/// subscription is sent one as it starts, and again whenever it has fallen behind, or, where the hub can't list it
/// again, its connection closes, so the client follows everything again. Each copy in it replaces the client's, unless
/// the client's is newer. One of the level the client holds that's missing from it, and isn't newer
/// than its revision, is gone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum Listed {
    /// Every record but those of one session, with the observations sent at the hub's level. Boxed, since it's the
    /// largest by far.
    Hub(Box<Records>),
    /// The records of one session.
    Session(SessionRecords),
    /// Every observation sent at one workspace's level. The hub queues it before its reply to the `Subscribe` that
    /// started the subscription, so a client that has the reply with no `Listed::Workspace` before it knows the hub
    /// refused that workspace, such as one it no longer has.
    Workspace(WorkspaceObservations),
    /// The observations of one issue or pull request, as a chip draws it.
    Reference(ReferenceObservations),
    /// Of a level from a newer hub, which this client skips.
    #[serde(other)]
    Unknown,
}

/// The records of one session, read in one transaction: its subagents, its proposals and how each of its replies said
/// out loud ended, with every observation sent at the session's level, read at the same moment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionRecords {
    pub session_id: SessionId,
    /// Oldest first.
    pub subagents: Vec<Subagent>,
    /// Oldest first.
    pub proposals: Vec<Proposal>,
    /// In no order, since each is read by its reply.
    pub replies_heard: Vec<ReplyHeard>,
    /// In no order. A kind it lacks is not known. Empty to a client of a version that reads none at it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<Observation>,
    /// The hub's newest revision when they were read.
    pub rev: Rev,
}

/// Every observation the hub holds at one workspace's level, read at one moment, of every kind. A kind it lacks is not
/// known. One the client holds of this level that the list lacks, and that isn't newer than `rev`, is gone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceObservations {
    pub workspace_id: WorkspaceId,
    /// In no order.
    pub observations: Vec<Observation>,
    /// The hub's newest revision when they were read.
    pub rev: Rev,
}

/// The observations of one issue or pull request, as a subscription that follows it is sent them: what GitHub says of
/// it, once the hub has read it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReferenceObservations {
    pub reference: ReferenceAsk,
    /// Empty until the hub has read it, which a client reads as not known.
    pub observations: Vec<Observation>,
    /// The hub's newest revision when they were read.
    pub rev: Rev,
}
