use protocol::model::{FoundGithubLogin, GhLogin, ProviderName};
use rusqlite::{Connection, Row, params};

use super::super::{Store, StoreError, json_at, lock, rev_of};

/// The columns every read of a found login's row takes, in this order, as [`found_at`] reads them.
const COLUMNS: &str = "provider, login, rev FROM found_github_logins";

/// A row's record. A login this build can't read, which only a build that wrote another form could leave, and only
/// until the hub starts again, reads as [`GhLogin::Unknown`].
fn found_at(row: &Row<'_>) -> rusqlite::Result<FoundGithubLogin> {
    Ok(FoundGithubLogin {
        provider: row.get::<_, String>(0)?.into(),
        login: json_at(row, 1)?.unwrap_or(GhLogin::Unknown),
        rev: rev_of(row, 2)?,
    })
}

/// Every found login, by provider, for a list of every record.
pub(in super::super) fn found_github_logins(conn: &Connection) -> rusqlite::Result<Vec<FoundGithubLogin>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {COLUMNS} ORDER BY provider"))?;
    statement.query_map([], found_at)?.collect()
}

impl Store {
    /// Records that `provider`'s machines' own `gh` is logged in as `login`, and returns the record, whose revision
    /// moves only when that's another login than the one recorded.
    pub(crate) fn record_found_github_login(
        &self,
        provider: &ProviderName,
        login: &GhLogin,
    ) -> Result<FoundGithubLogin, StoreError> {
        let login = serde_json::to_string(login)?;
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO found_github_logins (provider, login) VALUES (?1, ?2)
                 ON CONFLICT (provider) DO UPDATE SET login = excluded.login WHERE login != excluded.login",
            params![provider.as_str(), login],
        )?;
        Ok(conn.query_row(&format!("SELECT {COLUMNS} WHERE provider = ?"), [provider.as_str()], found_at)?)
    }

    /// Forgets every login found, as the hub starts, since it asks each provider again, and one taken out of its
    /// providers since it last ran isn't asked at all.
    pub(crate) fn forget_found_github_logins(&self) -> Result<(), StoreError> {
        lock(&self.writer).execute("DELETE FROM found_github_logins", [])?;
        Ok(())
    }
}
