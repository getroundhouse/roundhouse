use super::super::{ByColumn, Columns, Store, StoreError, columns, lock, place_of};
use protocol::event::EventBody;
use protocol::find::{Found, Needle};
use protocol::model::{Place, SessionId, SubagentId};

impl Store {
    /// Every event of the agent's own, or of the subagent `subagent`, that holds `needle`, by place, oldest
    /// first, as [`Needle::is_in_event`] says, read in one statement, so what it says is as of one moment. An event's
    /// body is only read as an event when its text may hold the needle, which most don't, so a long chat costs about as
    /// much as reading its text twice. The agent's raw lines aren't read at all. It reads on a connection of its own,
    /// so other reads don't wait for it.
    pub fn find_events(
        &self,
        session_id: &SessionId,
        subagent: Option<&SubagentId>,
        needle: &Needle,
    ) -> Result<Found, StoreError> {
        let conn = lock(&self.finder);
        columns! {
            struct FindColumns { place, body }
        }
        let c = FindColumns::at(0);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM events WHERE session_id = ? AND subagent IS ? ORDER BY place",
            FindColumns::LIST
        ))?;
        let mut rows = statement.query(rusqlite::params![session_id.as_str(), subagent.map(SubagentId::as_str)])?;
        let mut found = Found { places: Vec::new(), through: Place::default() };
        while let Some(row) = rows.next()? {
            let place = place_of(row, c.place)?;
            found.through = place;
            // Borrowed rather than copied, since most bodies are passed over.
            let body = row.borrowed_at(c.body)?.as_str().map_err(rusqlite::Error::from)?;
            if needle.may_be_in_json(body) && needle.is_in_event(&serde_json::from_str::<EventBody>(body)?) {
                found.places.push(place);
            }
        }
        Ok(found)
    }
}
