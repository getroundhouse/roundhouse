use super::super::{OneValue, Store, StoreError, lock};
use protocol::model::SessionId;
use rusqlite::OptionalExtension;

/// The build named by a chat's newest `AgentStarted`.
pub(in crate::store) const NEWEST_START: &str = "SELECT json_extract(body, '$.build') FROM events WHERE session_id = ? AND kind = 'agent_started' \
     ORDER BY seq DESC LIMIT 1";

impl Store {
    /// The build the session's agent last started on, as its newest `AgentStarted` says. `None` before its first, and
    /// when that didn't say, as from a hub from before it recorded builds.
    pub fn last_agent_build(&self, id: &SessionId) -> Result<Option<String>, StoreError> {
        let build = lock(&self.reader).query_row(NEWEST_START, [id.as_str()], OneValue::read).optional()?;
        Ok(build.flatten())
    }
}
