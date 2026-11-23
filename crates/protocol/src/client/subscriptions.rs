//! The requests that start and stop subscriptions. [`Subscribe`] follows anything, at any level, and the rest are its
//! forerunners, which follow a session's log or the hub's feed of its records, kept for clients of version 3 of the
//! client protocol. The client picks each `subscription_id`, because what's followed can arrive before the reply does.

use serde::{Deserialize, Serialize};

use super::{ApiError, ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Level, Place, ReferenceAsk, Seq, SessionId, SubagentId, TerminalId, WorkspaceId};

/// Starts and stops subscriptions. Each follows one thing at one [`Level`], under an ID the client picks, which every
/// frame for it carries. For each subscription started, the hub sends where the thing stands, and then every change, as
/// [`Follow`] says, until it's stopped or the connection ends. Stops go first, then starts, in order. A hub takes one
/// connection's `Subscribe` requests in the order they were sent, so a client needn't wait for one's reply before it
/// sends the next. Safe to send twice: starting an ID that already follows the same level and thing changes nothing,
/// and sends nothing again, and stopping one that isn't there does nothing. Answered with [`Subscribed`]. A hub from
/// before this refuses it as unsupported, and every hub that speaks version 4 of the client protocol has it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Subscribe {
    /// Stops a subscription of any kind, including one a request from before this started, as
    /// [`SessionUnsubscribe`] does.
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
    /// Whether the client reads what GitHub says as observations: at the hub's level, how each workspace's pull request
    /// stands, at a workspace's, its pull request's checks, history and stack and whether GitHub showed its repository,
    /// and at an issue's or a pull request's, what a chip draws. The hub sends them in place of `HubFrame::Github`.
    /// Where it's false, as every client from before it sends, the hub sends `HubFrame::Github` as [`Follow`] says, and
    /// none of GitHub's observations in any list or notice. A hub from before it ignores it, and a hub from before it
    /// sent a workspace's pull request as observations sends `HubFrame::Github` at the hub's level, and nothing of
    /// GitHub's at a workspace's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub github_observed: bool,
}

impl Subscription {
    /// A start of `follow` under `subscription_id`, reading what GitHub says as `HubFrame::Github`.
    pub fn new(subscription_id: u64, follow: Follow) -> Self {
        Self { subscription_id, follow, github_observed: false }
    }
}

/// What a subscription follows, with what each level needs to start it: for a session, where in its log.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum Follow {
    /// Sends a [`Listed::Hub`](super::listed::Listed::Hub), then every notice the feed sends at the hub's level, as
    /// [`HubNotice::level`](super::HubNotice::level) says, with how each workspace's own pull request stands among its
    /// observations, to a start that reads GitHub so, as [`Subscription::github_observed`] says. To any other, every
    /// one of those but GitHub's, and a `HubFrame::Github` for each workspace's own pull request, then again as it
    /// changes, as a [`GithubSubscribe`](super::github::GithubSubscribe) that showed every workspace would be sent.
    Hub,
    /// Sends a [`Listed::Workspace`](super::listed::Listed::Workspace), queued before the reply to the `Subscribe`
    /// that starts it, then every notice the feed sends at the workspace's level, its pull request's checks, history
    /// and stack among them only to a start that reads GitHub so, as [`Subscription::github_observed`] says. And has
    /// the hub read the workspace's pull request at once, and then as often as one shown in full is read, as a
    /// `GithubSubscribe` that shows it `closely` does.
    Workspace { id: WorkspaceId },
    /// Sends a [`Listed::Session`](super::listed::Listed::Session), then the events from `from`, then the tail, and then
    /// every event and delta, as [`SessionSubscribe`] and [`SessionSubscribePlaces`] do, with every notice the feed
    /// sends at the session's level.
    Session {
        id: SessionId,
        /// How far the client has the session's log, and so where the events it's sent start: after a reconnect, one
        /// past the last it received. `None` from its first event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<LogFrom>,
    },
    /// Sends what the terminal shows, as [`TerminalWatch`](super::terminals::TerminalWatch) does.
    Terminal { id: TerminalId },
    /// Has the hub read the issue or pull request again once it's stale, as a `GithubSubscribe` that shows it as a
    /// chip does. To a start that reads GitHub as observations, as [`Subscription::github_observed`] says, sends a
    /// [`Listed::Reference`](super::listed::Listed::Reference), then each change to the observations it lists. To
    /// any other, sends a `HubFrame::Github` of what GitHub says of it once the hub has read it, and again as that
    /// changes, as a `GithubSubscribe` sends them for its `references`.
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
    /// Every event, from `seq` on, as [`SessionSubscribe`] sends them.
    Seq { seq: Seq },
    /// Only the agent's own events, or only the subagent `subagent`'s, from `place` on, as [`SessionSubscribePlaces`]
    /// sends them. The tail and deltas are only what that one is writing.
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

/// Sends every event from `from_seq` on, then keeps streaming as new ones are recorded. To catch up after
/// reconnecting, subscribe from one past the last `seq` received.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionSubscribe {
    pub id: SessionId,
    pub from_seq: Seq,
    pub subscription_id: u64,
}

impl Request for SessionSubscribe {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionSubscribe(self)
    }
}

/// Follows the agent's own events, or one subagent's, from `from_place` on, as [`SessionSubscribe`] follows every
/// event. The tail and deltas are only what that one is writing. To catch up after reconnecting, subscribe from one
/// past the last place received. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionSubscribePlaces {
    pub id: SessionId,
    pub subagent: Option<SubagentId>,
    pub from_place: Place,
    pub subscription_id: u64,
}

impl Request for SessionSubscribePlaces {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionSubscribePlaces(self)
    }
}

/// Works for any kind of subscription.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionUnsubscribe {
    pub subscription_id: u64,
}

impl Request for SessionUnsubscribe {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionUnsubscribe(self)
    }
}

/// Sends a [`super::HubNotice`] whenever a record changes. Subscribe, then list with
/// [`RecordsList`](super::records::RecordsList), and keep whichever copy of each record has the newer revision. List
/// again on `Resync`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FeedSubscribe {
    pub subscription_id: u64,
}

impl Request for FeedSubscribe {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::FeedSubscribe(self)
    }
}
