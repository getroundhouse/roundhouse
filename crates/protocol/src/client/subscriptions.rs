//! The requests that follow a session's log, or the hub's feed of its records, and stop following either. The client
//! picks each `subscription_id`, because what's followed can arrive before the reply does. Each is answered with `Ok`:
//! one that follows once it's going, and one that stops whether or not anything was.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, Request};
use crate::model::{Place, Seq, SessionId, ToolUseId};

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
    pub subagent: Option<ToolUseId>,
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
