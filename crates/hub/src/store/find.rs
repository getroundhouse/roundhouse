//! Finding text in a chat: which of a session's events hold it.

use protocol::event::EventBody;
use protocol::find::{Found, Needle};
use protocol::model::{Place, SessionId, ToolUseId};

use super::{Store, StoreError, lock};

#[cfg(test)]
mod tests;

impl Store {
    /// Every event of the agent's own, or of the subagent `subagent` started, that holds `needle`, by place, oldest
    /// first, as [`Needle::is_in_event`] says, read in one statement, so what it says is as of one moment. An event's
    /// body is only read as an event when its text may hold the needle, which most don't, so a long chat costs about as
    /// much as reading its text twice. The agent's raw lines aren't read at all. It reads on a connection of its own,
    /// so other reads don't wait for it.
    pub fn find_events(
        &self,
        session_id: &SessionId,
        subagent: Option<&ToolUseId>,
        needle: &Needle,
    ) -> Result<Found, StoreError> {
        let conn = lock(&self.finder);
        let mut statement = conn
            .prepare_cached("SELECT place, body FROM events WHERE session_id = ? AND subagent IS ? ORDER BY place")?;
        let mut rows = statement.query(rusqlite::params![session_id.as_str(), subagent.map(ToolUseId::as_str)])?;
        let mut found = Found { places: Vec::new(), through: Place::default() };
        while let Some(row) = rows.next()? {
            let place = Place::new(row.get::<_, i64>(0)?.cast_unsigned());
            found.through = place;
            // Borrowed rather than copied, since most bodies are passed over.
            let body = row.get_ref(1)?.as_str().map_err(rusqlite::Error::from)?;
            if needle.may_be_in_json(body) && needle.is_in_event(&serde_json::from_str::<EventBody>(body)?) {
                found.places.push(place);
            }
        }
        Ok(found)
    }
}
