//! What the run that's running claims, which the session's record holds only while it runs.

use protocol::event::EventBody;
use protocol::model::{RunCapabilities, RunId, SessionId};
use rusqlite::{Connection, OptionalExtension, params};

use super::{Store, StoreError, lock};

/// For a run's exit, takes its claims off the session's record, which holds those of the run that's running and nothing
/// once it's gone, so a client offers a chat with no agent its kind's. An exit the hub records with no run is of
/// whichever ran, as at a start after the agent went with the machine. Returns the run whose claims it took off, which
/// the session announces.
pub(super) fn clear_claims(
    conn: &Connection,
    session_id: &SessionId,
    run: Option<RunId>,
    body: &EventBody,
) -> Result<Option<RunId>, StoreError> {
    let EventBody::AgentExited { .. } = body else { return Ok(None) };
    let params = params![session_id.as_str(), run.map(|run| run.get().cast_signed())];
    let held: Option<i64> = conn
        .query_row(
            "SELECT json_extract(agent_capabilities, '$.run') FROM sessions
             WHERE id = ?1 AND agent_capabilities IS NOT NULL
               AND (?2 IS NULL OR json_extract(agent_capabilities, '$.run') = ?2)",
            params,
            |row| row.get(0),
        )
        .optional()?;
    let Some(held) = held else { return Ok(None) };
    conn.execute("UPDATE sessions SET agent_capabilities = NULL WHERE id = ?", [session_id.as_str()])?;
    Ok(Some(RunId::new(held.cast_unsigned())))
}

/// Puts `claims` on the session's record, as those of the run that's running.
pub(super) fn set_claims(
    conn: &Connection,
    session_id: &SessionId,
    claims: &RunCapabilities,
) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE sessions SET agent_capabilities = ? WHERE id = ?",
        params![serde_json::to_string(claims)?, session_id.as_str()],
    )?;
    Ok(())
}

impl Store {
    /// Puts `claims` on the session's record, for a run that's found running without a start to record them with.
    pub fn set_claims(&self, session_id: &SessionId, claims: &RunCapabilities) -> Result<(), StoreError> {
        set_claims(&lock(&self.writer), session_id, claims)
    }

    /// Takes the claims off the record of every session but `running`'s, whose agents are the only ones running, as
    /// a hub learns as it starts, since an agent that went while no hub was there left no exit to take them off.
    pub fn clear_claims_but(&self, running: &[SessionId]) -> Result<(), StoreError> {
        let running = serde_json::to_string(running)?;
        lock(&self.writer).execute(
            "UPDATE sessions SET agent_capabilities = NULL
             WHERE agent_capabilities IS NOT NULL AND id NOT IN (SELECT value FROM json_each(?))",
            [running],
        )?;
        Ok(())
    }
}
