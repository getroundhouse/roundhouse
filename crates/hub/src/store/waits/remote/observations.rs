use super::{COLUMNS, Store, StoreError, Wait, WorkspaceId, lock, params, text_of, wait_from_row};

impl Store {
    /// Deadline expiry reads only due rows, rather than deserializing every open observation on each timer tick.
    pub fn expire_remote_waits(&self, at_ms: i64) -> Result<Vec<Wait>, StoreError> {
        let conn = lock(&self.writer);
        let mut query = conn.prepare_cached(&format!(
            "UPDATE waits SET ended_at_ms = ?1, ended = 'timed_out' WHERE ended_at_ms IS NULL AND watch IS NOT NULL
             AND json_extract(watch, '$.deadline_at_ms') <= ?1 RETURNING {COLUMNS}"
        ))?;
        Ok(query.query_map([at_ms], wait_from_row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Durable open tasks. The scheduler is recreated after a restart, making their groups immediately due.
    pub fn remote_waits(&self) -> Result<Vec<Wait>, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM waits WHERE ended_at_ms IS NULL AND watch IS NOT NULL ORDER BY started_at_ms, id"
        ))?;
        Ok(query.query_map([], wait_from_row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies one observation, error or deadline to each matching open wait. The writer lock also serializes
    /// cancellation, so a terminal result can never be overwritten by a late read.
    pub fn revise_remote_waits(
        &self,
        workspace: Option<&WorkspaceId>,
        mut revise: impl FnMut(&mut Wait),
    ) -> Result<Vec<Wait>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let waits = tx.prepare_cached(&format!("SELECT {COLUMNS} FROM waits WHERE ended_at_ms IS NULL AND watch IS NOT NULL AND (?1 IS NULL OR workspace_id = ?1) ORDER BY started_at_ms, id"))?
            .query_map([workspace.map(WorkspaceId::as_str)], wait_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut changed = Vec::new();
        for mut wait in waits {
            let before = wait.clone();
            revise(&mut wait);
            if wait == before {
                continue;
            }
            tx.execute(
                "UPDATE waits SET watch = ?, ended_at_ms = ?, ended = ? WHERE id = ? AND ended_at_ms IS NULL",
                params![
                    serde_json::to_string(&wait.watch)?,
                    wait.ended.map(|end| end.at_ms),
                    wait.ended.map(|end| text_of(&end.how)),
                    wait.id.as_str()
                ],
            )?;
            changed.push(wait);
        }
        tx.commit()?;
        Ok(changed)
    }
}
