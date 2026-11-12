//! The hub's own settings, which are one record, and who the hub is.

use protocol::model::{HubId, HubSettings, HubSettingsChange};
use rusqlite::{Connection, params};

use super::{Store, StoreError, lock, rev_of};

#[cfg(test)]
mod tests;

pub(super) fn hub_settings(conn: &Connection) -> rusqlite::Result<HubSettings> {
    conn.query_row("SELECT keep_awake, rev FROM hub_settings", [], |row| {
        Ok(HubSettings { keep_awake: row.get(0)?, rev: rev_of(row, 1)? })
    })
}

impl Store {
    /// This hub's ID, made when its database was.
    pub fn hub_id(&self) -> Result<HubId, StoreError> {
        let id: String = lock(&self.reader).query_row("SELECT hub_id FROM hub_identity", [], |row| row.get(0))?;
        Ok(id.as_str().into())
    }

    pub fn hub_settings(&self) -> Result<HubSettings, StoreError> {
        Ok(hub_settings(&lock(&self.reader))?)
    }

    /// Changes the settings `change` names. Only a setting that ends up different gives the record a newer revision,
    /// so setting the same again says nothing new.
    pub fn change_hub_settings(&self, change: &HubSettingsChange) -> Result<HubSettings, StoreError> {
        let conn = lock(&self.writer);
        if let Some(keep_awake) = change.keep_awake {
            conn.execute("UPDATE hub_settings SET keep_awake = ?1 WHERE keep_awake != ?1", params![keep_awake])?;
        }
        Ok(hub_settings(&conn)?)
    }
}
