//! The calls for the agent's own events, or one subagent's, which are read and followed by place.

use protocol::client::{ClientRequest, ClientResponse};
use protocol::event::SessionEvent;
use protocol::model::{Place, SessionId, ToolUseId};

use super::{Client, Result, Subscription, unexpected};

impl Client {
    /// Returns the agent's own events, or those of the subagent `subagent` started, from `from_place` on, and the place
    /// of the newest of them. A hub from before places refuses it, which [`super::ClientError::is_unsupported`] says.
    /// See [`ClientRequest::SessionReadPlaces`].
    pub fn session_read_places(
        &self,
        id: &SessionId,
        subagent: Option<&ToolUseId>,
        from_place: Place,
        limit: u32,
    ) -> Result<(Vec<SessionEvent>, Place)> {
        let request = ClientRequest::SessionReadPlaces {
            id: id.clone(),
            subagent: subagent.cloned(),
            from_place,
            limit,
            max_bytes: None,
            include_raw: false,
        };
        let reply = self.call(request)?;
        let ClientResponse::PlacedEvents { events, last_place } = reply else {
            return unexpected(&reply);
        };
        Ok((events, last_place))
    }

    /// Follows the agent's own events, or one subagent's, by place. See [`ClientRequest::SessionSubscribePlaces`].
    pub fn session_subscribe_places(
        &self,
        id: &SessionId,
        subagent: Option<&ToolUseId>,
        from_place: Place,
    ) -> Result<Subscription> {
        let subagent = subagent.cloned();
        self.subscribe(|subscription_id| ClientRequest::SessionSubscribePlaces {
            id: id.clone(),
            subagent,
            from_place,
            subscription_id,
        })
    }
}
