//! What the store reads only for a worker of protocol 3, which goes with `protocol::worker::previous`, at the next
//! break.

use protocol::capability::Capabilities;
use protocol::model::{RunId, SessionId};
use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError, lock};

impl Store {
    /// What `run` said it claims, as the newest of its events that said, which the log keeps after its exit. A launch
    /// of protocol 3 that finds its run running reads them here, since its reply doesn't say them.
    pub fn claims_said(&self, session_id: &SessionId, run: RunId) -> Result<Option<Capabilities>, StoreError> {
        let select = "SELECT json_extract(body, '$.capabilities') FROM events
                      WHERE session_id = ? AND run = ? AND kind IN ('agent_started', 'agent_init')
                        AND json_extract(body, '$.capabilities') IS NOT NULL
                      ORDER BY seq DESC LIMIT 1";
        let params = params![session_id.as_str(), run.get().cast_signed()];
        let said: Option<String> = lock(&self.reader).query_row(select, params, |row| row.get(0)).optional()?;
        Ok(said.map(|claims| serde_json::from_str(&claims)).transpose()?)
    }
}
