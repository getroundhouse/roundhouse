use super::super::{Store, StoreError, from_text, lock, rev_of, text_of};
use protocol::model::{AccountDefault, AccountId, AgentKind, HubId, HubSettings, HubSettingsChange};
use rusqlite::{Connection, params};

pub(in super::super) fn hub_settings(conn: &Connection) -> rusqlite::Result<HubSettings> {
    conn.query_row("SELECT keep_awake, allow_asked_in_full_access, rev FROM hub_settings", [], |row| {
        // The outer SELECT remains active while the defaults are read, keeping one SQLite snapshot.
        let mut defaults = conn.prepare_cached("SELECT agent, account_id FROM account_defaults ORDER BY agent")?;
        let account_defaults = defaults
            .query_map([], |row| {
                Ok(AccountDefault { agent: from_text(row.get(0)?)?, account: row.get::<_, String>(1)?.into() })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(HubSettings {
            account_defaults: Some(account_defaults),
            keep_awake: row.get(0)?,
            allow_asked_in_full_access: Some(row.get(1)?),
            rev: rev_of(row, 2)?,
        })
    })
}

impl Store {
    /// Called with the logins lock held, after checking the account and its token.
    pub(crate) fn set_account_default(&self, agent: AgentKind, account: &AccountId) -> Result<HubSettings, StoreError> {
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO account_defaults (agent, account_id) VALUES (?1, ?2)
                ON CONFLICT (agent) DO UPDATE SET account_id = excluded.account_id
                WHERE account_id != excluded.account_id",
                params![text_of(&agent), account.as_str()],
            )?;
            hub_settings(tx)
        })
    }

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
        if let Some(allow) = change.allow_asked_in_full_access {
            let sql = "UPDATE hub_settings SET allow_asked_in_full_access = ?1 WHERE allow_asked_in_full_access != ?1";
            conn.execute(sql, params![allow])?;
        }
        Ok(hub_settings(&conn)?)
    }
}
