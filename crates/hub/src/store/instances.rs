//! The hub's record of each instance it asks a provider for. It's written before the provider is asked, so the hub
//! knows which instances are its own whatever a provider lists, and can finish making or destroying one that a crash
//! cut short. Whether an instance exists, and whether it's running, is the provider's to say.

use protocol::model::InstanceId;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};

use super::{Store, StoreError, from_text, lock, text_of};

#[cfg(test)]
mod tests;

/// How far the hub has got with an instance.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStage {
    /// The hub is asking its provider to make it, or was when it stopped.
    Creating,
    /// Its provider made it.
    Ready,
    /// The hub is asking its provider to destroy it, or was when it stopped.
    Destroying,
}

/// An instance the hub asked a provider for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedInstance {
    pub id: InstanceId,
    /// The provider's name, as the hub's providers are named.
    pub provider: String,
    pub stage: InstanceStage,
    pub created_at_ms: i64,
}

const INSTANCE_COLUMNS: &str = "id, provider, stage, created_at_ms";

fn instance_from_row(row: &Row<'_>) -> rusqlite::Result<RecordedInstance> {
    Ok(RecordedInstance {
        id: row.get::<_, String>(0)?.into(),
        provider: row.get(1)?,
        stage: from_text(row.get(2)?)?,
        created_at_ms: row.get(3)?,
    })
}

/// Records an instance the hub is about to ask `provider` for, or has. One already recorded is a mistake, such as two
/// workspaces given one instance, and fails.
pub(super) fn record_instance(
    conn: &Connection,
    id: &InstanceId,
    provider: &str,
    stage: InstanceStage,
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        &format!("INSERT INTO instances ({INSTANCE_COLUMNS}) VALUES (?, ?, ?, ?)"),
        params![id.as_str(), provider, text_of(&stage), created_at_ms],
    )?;
    Ok(())
}

impl Store {
    /// Its provider made it.
    pub fn instance_made(&self, id: &InstanceId) -> Result<(), StoreError> {
        let (creating, ready) = (text_of(&InstanceStage::Creating), text_of(&InstanceStage::Ready));
        lock(&self.writer).execute(
            "UPDATE instances SET stage = ? WHERE id = ? AND stage = ?",
            params![ready, id.as_str(), creating],
        )?;
        Ok(())
    }

    /// Before its provider is asked to destroy it, so a destroy cut short is finished later.
    pub fn instance_destroying(&self, id: &InstanceId) -> Result<(), StoreError> {
        let destroying = text_of(&InstanceStage::Destroying);
        lock(&self.writer).execute("UPDATE instances SET stage = ? WHERE id = ?", params![destroying, id.as_str()])?;
        Ok(())
    }

    /// Once its provider says it's gone.
    pub fn instance_destroyed(&self, id: &InstanceId) -> Result<(), StoreError> {
        lock(&self.writer).execute("DELETE FROM instances WHERE id = ?", [id.as_str()])?;
        Ok(())
    }

    /// What the last run left part way, as it starts, before anything new is made. One it was making is of no use
    /// now, since what it was for was left half-made too, so it's marked to be destroyed, with one it was destroying.
    pub fn take_unfinished_instances(&self) -> Result<Vec<RecordedInstance>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let (creating, destroying) = (text_of(&InstanceStage::Creating), text_of(&InstanceStage::Destroying));
        tx.execute("UPDATE instances SET stage = ?1 WHERE stage = ?2", params![destroying, creating])?;
        let unfinished = {
            let mut statement = tx.prepare_cached(&format!(
                "SELECT {INSTANCE_COLUMNS} FROM instances WHERE stage = ? ORDER BY created_at_ms, id"
            ))?;
            let rows = statement.query_map([&destroying], instance_from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(unfinished)
    }

    /// Every instance the hub has recorded, oldest first.
    pub fn recorded_instances(&self) -> Result<Vec<RecordedInstance>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached(&format!("SELECT {INSTANCE_COLUMNS} FROM instances ORDER BY created_at_ms, id"))?;
        let rows = statement.query_map([], instance_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }
}
