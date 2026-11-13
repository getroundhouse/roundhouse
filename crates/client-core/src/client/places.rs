//! The calls for the agent's own events, or one subagent's, which are read and followed by place.

use protocol::client::{log, subscriptions};
use protocol::event::SessionEvent;
use protocol::model::{Place, SessionId, ToolUseId};

use super::{Client, Result, Subscription};

impl Client {
    /// Returns the agent's own events, or those of the subagent `subagent` started, from `from_place` on, and the place
    /// of the newest of them. A hub from before places refuses it, which [`super::ClientError::is_unsupported`] says.
    /// See [`log::SessionReadPlaces`].
    pub fn session_read_places(
        &self,
        id: &SessionId,
        subagent: Option<&ToolUseId>,
        from_place: Place,
        limit: u32,
    ) -> Result<(Vec<SessionEvent>, Place)> {
        let read = log::SessionReadPlaces {
            id: id.clone(),
            subagent: subagent.cloned(),
            from_place,
            limit,
            max_bytes: None,
            include_raw: false,
        };
        let log::PlacedEvents { events, last_place } = self.call(read)?;
        Ok((events, last_place))
    }

    /// Follows the agent's own events, or one subagent's, by place. See [`subscriptions::SessionSubscribePlaces`].
    pub fn session_subscribe_places(
        &self,
        id: &SessionId,
        subagent: Option<&ToolUseId>,
        from_place: Place,
    ) -> Result<Subscription> {
        let subagent = subagent.cloned();
        self.subscribe(|subscription_id| subscriptions::SessionSubscribePlaces {
            id: id.clone(),
            subagent,
            from_place,
            subscription_id,
        })
    }
}
