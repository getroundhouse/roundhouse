//! The calls for the agent's own events, or one subagent's, which are read and followed by place.

use protocol::client::log;
use protocol::event::SessionEvent;
use protocol::model::{Place, SessionId, SubagentId};

use super::{Client, Result};

impl Client {
    /// Returns the agent's own events, or those of the subagent `subagent`, from `from_place` on, and the place
    /// of the newest of them. See [`log::SessionReadPlaces`].
    pub fn session_read_places(
        &self,
        id: &SessionId,
        subagent: Option<&SubagentId>,
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
}
