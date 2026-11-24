//! The request that starts and stops subscriptions, [`Subscribe`], which follows anything, at any level. The client
//! picks each `subscription_id`, because what's followed can arrive before the reply does.

use serde::{Deserialize, Serialize};

use super::{ApiError, ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Level, Place, ReferenceAsk, Seq, SessionId, SubagentId, TerminalId, WorkspaceId};

/// Starts and stops subscriptions. Each follows one thing at one [`Level`], under an ID the client picks, which every
/// frame for it carries. For each subscription started, the hub sends where the thing stands, and then every change, as
/// [`Follow`] says, until it's stopped or the connection ends. Stops go first, then starts, in order. A hub takes one
/// connection's `Subscribe` requests in the order they were sent, so a client needn't wait for one's reply before it
/// sends the next. Safe to send twice: starting an ID that already follows the same level and thing changes nothing,
/// and sends nothing again, and stopping one that isn't there does nothing. Answered with [`Subscribed`].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Subscribe {
    /// Stops a subscription.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start: Vec<Subscription>,
}

impl Request for Subscribe {
    type Reply = Subscribed;

    fn into_request(self) -> ClientRequest {
        ClientRequest::Subscribe(self)
    }
}

/// One subscription to start: what it follows, and the ID its frames carry. On the wire, what it follows sits beside
/// the ID: `{"subscription_id":7,"level":"session","id":"…","from":{"by":"seq","seq":120}}`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Subscription {
    pub subscription_id: u64,
    #[serde(flatten)]
    pub follow: Follow,
}

impl Subscription {
    /// A start of `follow` under `subscription_id`.
    pub fn new(subscription_id: u64, follow: Follow) -> Self {
        Self { subscription_id, follow }
    }
}

/// What a subscription follows, with what each level needs to start it: for a session, where in its log.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum Follow {
    /// Sends a [`Listed::Hub`](super::listed::Listed::Hub), then every notice the feed sends at the hub's level, as
    /// [`HubNotice::level`](super::HubNotice::level) says, with how each workspace's own pull request stands among its
    /// observations.
    Hub,
    /// Sends a [`Listed::Workspace`](super::listed::Listed::Workspace), queued before the reply to the `Subscribe`
    /// that starts it, then every notice the feed sends at the workspace's level, its pull request's checks, history
    /// and stack among them. And has the hub read the workspace's pull request at once, and then as often as one the
    /// pull request's bar shows.
    Workspace { id: WorkspaceId },
    /// Sends a [`Listed::Session`](super::listed::Listed::Session), then the events from `from`, then the tail, and then
    /// every event and delta, with every notice the feed sends at the session's level.
    Session {
        id: SessionId,
        /// How far the client has the session's log, and so where the events it's sent start: after a reconnect, one
        /// past the last it received. `None` from its first event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<LogFrom>,
    },
    /// Sends what the terminal shows, as frames of `HubFrame::TerminalPicture`. The first is fresh and whole, from the
    /// hub's copy of the screen if it has one, and each after it has only what the client lacks of the newest the hub
    /// has, with at most one frame on its way at a time, so a client on a slow link is sent the newest of what the
    /// terminal shows rather than all that came between. Its shell's end comes as a `HubFrame::Terminal` with
    /// [`TerminalUpdate::Exited`](crate::terminal::TerminalUpdate::Exited), after which nothing more does.
    Terminal { id: TerminalId },
    /// Has the hub read the issue or pull request again once it's stale, as it does what a chip shows, and sends a
    /// [`Listed::Reference`](super::listed::Listed::Reference), then each change to the observations it lists.
    Reference { reference: ReferenceAsk },
    /// From a newer client. Refused as unsupported.
    #[serde(other)]
    Unknown,
}

impl Follow {
    /// The level, and which thing at it, that this follows.
    pub fn level(&self) -> Level {
        match self {
            Follow::Hub => Level::Hub,
            Follow::Workspace { id } => Level::Workspace { id: id.clone() },
            Follow::Session { id, .. } => Level::Session { id: id.clone() },
            Follow::Terminal { id } => Level::Terminal { id: id.clone() },
            Follow::Reference { reference } => Level::Reference { reference: reference.clone() },
            Follow::Unknown => Level::Unknown,
        }
    }
}

/// Where in a session's log the events a subscription is sent start, by `seq` or by place.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum LogFrom {
    /// Every event, from `seq` on.
    Seq { seq: Seq },
    /// Only the agent's own events, or only the subagent `subagent`'s, from `place` on. The tail and deltas are only
    /// what that one is writing.
    Place {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent: Option<SubagentId>,
        place: Place,
    },
    /// From a newer client. Refused as unsupported.
    #[serde(other)]
    Unknown,
}

/// The reply to [`Subscribe`]: which of its starts the hub refused, each with why. Every start not listed here is
/// going, and every stop is done.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Subscribed {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refused: Vec<Refused>,
}

impl Reply for Subscribed {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Subscribed(subscribed) = response else { return Err(response) };
        Ok(subscribed)
    }
}

/// A start the hub didn't make: `Unsupported` for a level or a `from` it doesn't know, `NotFound` for a workspace or a
/// session it doesn't have or a terminal that isn't open, and `InvalidRequest` for an ID that follows something else,
/// or for more issues and pull requests on one connection than
/// [`MOST_REFERENCES_SHOWN`](super::github::MOST_REFERENCES_SHOWN).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Refused {
    pub subscription_id: u64,
    pub error: ApiError,
}
