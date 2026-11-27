use super::super::{ByColumn, Columns, OneValue, Store, StoreError, columns, lock, now_ms, text_at, text_of};
use protocol::dialing::TicketSecret;
use protocol::model::{InstanceId, ProviderName};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

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
    pub provider: ProviderName,
    pub stage: InstanceStage,
    pub created_at_ms: i64,
}

columns! {
    /// What an instance's row holds, as it's written and read.
    struct InstanceColumns { id, provider, stage, created_at_ms }
    written as InstanceValues
}

fn instance_from_row(row: &Row<'_>) -> rusqlite::Result<RecordedInstance> {
    let c = InstanceColumns::at(0);
    Ok(RecordedInstance {
        id: row.at::<String>(c.id)?.into(),
        provider: row.at::<String>(c.provider)?.into(),
        stage: text_at(row, c.stage)?,
        created_at_ms: row.at(c.created_at_ms)?,
    })
}

/// Records an instance the hub is about to ask `provider` for, or has. One already recorded is a mistake, such as two
/// workspaces given one instance, and fails.
pub(in super::super) fn record_instance(
    conn: &Connection,
    id: &InstanceId,
    provider: &ProviderName,
    stage: InstanceStage,
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    let values = InstanceValues {
        id: &id.as_str(),
        provider: &provider.as_str(),
        stage: &text_of(&stage),
        created_at_ms: &created_at_ms,
    };
    conn.execute(&format!("INSERT INTO instances {}", InstanceValues::INSERT), values.params())?;
    Ok(())
}

impl Store {
    /// Before its provider is asked to make it again, once it's been destroyed, as for an ephemeral workspace being
    /// restored. One still recorded fails, as with [`record_instance`].
    pub fn instance_creating(&self, id: &InstanceId, provider: &ProviderName) -> Result<(), StoreError> {
        record_instance(&lock(&self.writer), id, provider, InstanceStage::Creating, now_ms())?;
        Ok(())
    }

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

    /// Before the ticket whose secret is `secret` is given to its provider, so a worker can dial with it, as the hub
    /// restarts too. It replaces any ticket of the instance's that no worker has dialed with, so only the newest
    /// admits one. False for an instance that isn't recorded, or is being destroyed, which no ticket is for.
    pub fn instance_ticket(&self, id: &InstanceId, secret: &TicketSecret) -> Result<bool, StoreError> {
        let destroying = text_of(&InstanceStage::Destroying);
        let changed = lock(&self.writer).execute(
            "UPDATE instances SET ticket = ?1 WHERE id = ?2 AND stage != ?3",
            params![secret.as_str(), id.as_str(), destroying],
        )?;
        Ok(changed == 1)
    }

    /// The instance a worker that proved it holds `key` is admitted to: the one whose worker it was admitted as before,
    /// or else the one whose unused ticket's secret is `secret`, whose ticket is then used, and whose worker it is from
    /// then on, in place of any before it. `None` for neither, and for an instance being destroyed.
    pub fn admit_worker(&self, secret: &TicketSecret, key: &str) -> Result<Option<InstanceId>, StoreError> {
        let destroying = text_of(&InstanceStage::Destroying);
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let again: Option<String> = tx
            .query_row(
                "SELECT id FROM instances WHERE worker_key = ?1 AND stage != ?2",
                params![key, destroying],
                OneValue::read,
            )
            .optional()?;
        let admitted = match again {
            Some(id) => Some(id),
            None => tx
                .query_row(
                    "UPDATE instances SET ticket = NULL, worker_key = ?1 WHERE ticket = ?2 AND stage != ?3 RETURNING id",
                    params![key, secret.as_str(), destroying],
                    OneValue::read,
                )
                .optional()?,
        };
        tx.commit()?;
        Ok(admitted.map(InstanceId::from))
    }

    /// Whether any instance has a worker that dials the hub, or a ticket a worker could dial with.
    pub fn has_dialing_workers(&self) -> Result<bool, StoreError> {
        let conn = lock(&self.reader);
        let sql = "SELECT EXISTS (SELECT 1 FROM instances WHERE ticket IS NOT NULL OR worker_key IS NOT NULL)";
        Ok(conn.query_row(sql, [], OneValue::read)?)
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
                "SELECT {} FROM instances WHERE stage = ? ORDER BY created_at_ms, id",
                InstanceColumns::LIST
            ))?;
            let rows = statement.query_map([&destroying], instance_from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(unfinished)
    }

    /// The recorded instance `id`, if it hasn't been destroyed.
    pub fn recorded_instance(&self, id: &InstanceId) -> Result<Option<RecordedInstance>, StoreError> {
        let conn = lock(&self.reader);
        conn.query_row(
            &format!("SELECT {} FROM instances WHERE id = ?", InstanceColumns::LIST),
            [id.as_str()],
            instance_from_row,
        )
        .optional()
        .map_err(StoreError::from)
    }

    /// Every instance the hub has recorded, oldest first.
    pub fn recorded_instances(&self) -> Result<Vec<RecordedInstance>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn
            .prepare_cached(&format!("SELECT {} FROM instances ORDER BY created_at_ms, id", InstanceColumns::LIST))?;
        let rows = statement.query_map([], instance_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }
}
