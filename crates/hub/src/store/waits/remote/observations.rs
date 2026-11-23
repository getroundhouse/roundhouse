use super::super::super::Columns;
use super::{
    HashMap, HashSet, OneValue, OptionalExtension, Store, StoreError, Wait, WaitColumns, WaitId, WaitRead, WorkspaceId,
    lock, params, text_of, wait_from_row,
};

impl Store {
    /// Deadline expiry reads only due rows, rather than deserializing every open observation on each timer tick.
    pub fn expire_remote_waits(&self, at_ms: i64) -> Result<Vec<Wait>, StoreError> {
        let conn = lock(&self.writer);
        let mut query = conn.prepare_cached(&format!(
            "UPDATE waits SET ended_at_ms = ?1, ended = 'timed_out' WHERE ended_at_ms IS NULL AND watch IS NOT NULL
             AND json_extract(watch, '$.deadline_at_ms') <= ?1 RETURNING {}",
            WaitColumns::LIST
        ))?;
        Ok(query.query_map([at_ms], wait_from_row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Durable open tasks. The scheduler is recreated after a restart, making their groups immediately due.
    pub fn remote_waits(&self) -> Result<Vec<Wait>, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached(&format!(
            "SELECT {} FROM waits WHERE ended_at_ms IS NULL AND watch IS NOT NULL ORDER BY started_at_ms, id",
            WaitColumns::LIST
        ))?;
        Ok(query.query_map([], wait_from_row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies one read, failure or deadline to each matching open wait, as `revise` says, with what it last read,
    /// `reads`, which the hub keeps as an observation, in place of what its record keeps, which is none of it. One that
    /// isn't in `reads` is judged as its record has it, so nothing it read before can end it. A wait that ends keeps
    /// what it last read in its record from then, frozen in the same transaction. One that goes on keeps none, since
    /// the hub keeps it, and nothing else of its record changes as it's judged. Returns each that changed, with what it
    /// read. The writer lock also serializes cancellation, so a terminal result can never be overwritten by a late
    /// read.
    pub fn revise_remote_waits(
        &self,
        workspace: Option<&WorkspaceId>,
        reads: &HashMap<WaitId, WaitRead>,
        mut revise: impl FnMut(&mut Wait),
    ) -> Result<Vec<Wait>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let waits = tx.prepare_cached(&format!("SELECT {} FROM waits WHERE ended_at_ms IS NULL AND watch IS NOT NULL AND (?1 IS NULL OR workspace_id = ?1) ORDER BY started_at_ms, id", WaitColumns::LIST))?
            .query_map([workspace.map(WorkspaceId::as_str)], wait_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut changed = Vec::new();
        for mut wait in waits {
            if let (Some(watch), Some(read)) = (wait.watch.as_mut(), reads.get(&wait.id)) {
                **watch = watch.recorded().with_read(read.clone());
            }
            let before = wait.clone();
            revise(&mut wait);
            if wait == before {
                continue;
            }
            changed.push(wait.clone());
            // What a judging changes of one that goes on is what it read, which its record doesn't keep.
            if wait.ended.is_none() {
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
        }
        tx.commit()?;
        Ok(changed)
    }
}

impl Store {
    /// Keeps `read` as what the wait `id`, which has ended, last read, in its record, where the hub kept it apart while
    /// the wait was open. Nothing for a wait that hasn't ended, or has no watch.
    pub fn freeze_wait_read(&self, id: &WaitId, read: &WaitRead) -> Result<(), StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let ended = tx
            .prepare_cached(&format!(
                "SELECT {} FROM waits WHERE id = ? AND ended_at_ms IS NOT NULL AND watch IS NOT NULL",
                WaitColumns::LIST
            ))?
            .query_row([id.as_str()], wait_from_row)
            .optional()?;
        if let Some(watch) = ended.and_then(|wait| wait.watch) {
            let frozen = watch.with_read(read.clone());
            tx.execute(
                "UPDATE waits SET watch = ? WHERE id = ?",
                params![serde_json::to_string(&frozen)?, id.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every open wait's ID, so the hub forgets what it read of one that has ended.
    pub(crate) fn open_wait_ids(&self) -> Result<HashSet<WaitId>, StoreError> {
        let conn = lock(&self.reader);
        let mut query = conn.prepare_cached("SELECT id FROM waits WHERE ended_at_ms IS NULL")?;
        let ids = query.query_map([], OneValue::read::<String>)?;
        Ok(ids.map(|id| id.map(WaitId::from)).collect::<rusqlite::Result<_>>()?)
    }
}
