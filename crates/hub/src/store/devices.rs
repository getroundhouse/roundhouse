//! Devices paired with the hub, and the hashes of their tokens.

use protocol::model::DeviceId;
use protocol::pairing::{Device, DeviceToken};
use rusqlite::{OptionalExtension, Row, params};

use super::{Store, StoreError, lock};

/// A token's SHA-256, which is all the hub keeps of it. A token is 32 random bytes, so a hash with no salt is as
/// hard to reverse as guessing the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenHash([u8; 32]);

impl TokenHash {
    pub fn of(token: &DeviceToken) -> TokenHash {
        let digest = ring::digest::digest(&ring::digest::SHA256, token.as_str().as_bytes());
        let mut hash = [0; 32];
        hash.copy_from_slice(digest.as_ref());
        TokenHash(hash)
    }
}

const DEVICE_COLUMNS: &str = "id, name, paired_at_ms, last_seen_ms";

fn device_from_row(row: &Row<'_>) -> rusqlite::Result<Device> {
    Ok(Device {
        id: row.get::<_, String>(0)?.into(),
        name: row.get(1)?,
        paired_at_ms: row.get(2)?,
        last_seen_ms: row.get(3)?,
        connections: 0,
    })
}

impl Store {
    pub fn add_device(&self, device: &Device, token: &TokenHash) -> Result<(), StoreError> {
        lock(&self.writer).execute(
            &format!("INSERT INTO devices ({DEVICE_COLUMNS}, token_sha256) VALUES (?, ?, ?, ?, ?)"),
            params![device.id.as_str(), device.name, device.paired_at_ms, device.last_seen_ms, &token.0[..]],
        )?;
        Ok(())
    }

    /// The device this token belongs to, if one does.
    pub fn device_with_token(&self, token: &TokenHash) -> Result<Option<Device>, StoreError> {
        let query = format!("SELECT {DEVICE_COLUMNS} FROM devices WHERE token_sha256 = ?");
        Ok(lock(&self.reader).query_row(&query, [&token.0[..]], device_from_row).optional()?)
    }

    /// Whether the device is still paired.
    pub fn device_seen(&self, id: &DeviceId, at_ms: i64) -> Result<bool, StoreError> {
        let sql = "UPDATE devices SET last_seen_ms = ? WHERE id = ?";
        Ok(lock(&self.writer).execute(sql, params![at_ms, id.as_str()])? > 0)
    }

    /// In the order they were paired.
    pub fn list_devices(&self) -> Result<Vec<Device>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement =
            conn.prepare_cached(&format!("SELECT {DEVICE_COLUMNS} FROM devices ORDER BY paired_at_ms, id"))?;
        let rows = statement.query_map([], device_from_row)?;
        rows.collect::<rusqlite::Result<_>>().map_err(StoreError::from)
    }

    pub fn has_devices(&self) -> Result<bool, StoreError> {
        Ok(lock(&self.reader).query_row("SELECT EXISTS (SELECT 1 FROM devices)", [], |row| row.get(0))?)
    }

    /// Whether there was one to remove.
    pub fn remove_device(&self, id: &DeviceId) -> Result<bool, StoreError> {
        Ok(lock(&self.writer).execute("DELETE FROM devices WHERE id = ?", [id.as_str()])? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, paired_at_ms: i64) -> Device {
        Device {
            id: id.into(),
            name: format!("{id}'s phone"),
            paired_at_ms,
            last_seen_ms: paired_at_ms,
            connections: 0,
        }
    }

    #[test]
    fn a_device_is_found_by_its_token_until_it_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("hub.db"), &dir.path().join("backups")).unwrap();
        let token = DeviceToken::from("token");
        store.add_device(&device("a", 1), &TokenHash::of(&token)).unwrap();

        assert!(store.device_seen(&"a".into(), 5).unwrap());

        let found = store.device_with_token(&TokenHash::of(&token)).unwrap();
        assert_eq!(found, Some(Device { last_seen_ms: 5, ..device("a", 1) }));
        assert_eq!(store.device_with_token(&TokenHash::of(&DeviceToken::from("other"))).unwrap(), None);
        assert!(store.remove_device(&"a".into()).unwrap());
        assert_eq!(store.device_with_token(&TokenHash::of(&token)).unwrap(), None);
        assert!(!store.remove_device(&"a".into()).unwrap());
        assert!(!store.device_seen(&"a".into(), 6).unwrap());
        assert!(!store.has_devices().unwrap());
    }

    #[test]
    fn only_the_hash_of_a_token_is_stored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hub.db");
        let store = Store::open(&path, &dir.path().join("backups")).unwrap();

        store.add_device(&device("a", 1), &TokenHash::of(&DeviceToken::from("plain-token-text"))).unwrap();
        store.add_device(&device("b", 2), &TokenHash::of(&DeviceToken::from("another"))).unwrap();

        assert_eq!(store.list_devices().unwrap(), vec![device("a", 1), device("b", 2)]);
        drop(store);
        let bytes = std::fs::read(&path).unwrap();
        let wal = std::fs::read(path.with_extension("db-wal")).unwrap_or_default();
        let has = |haystack: &[u8]| haystack.windows(16).any(|window| window == b"plain-token-text");
        assert!(!has(&bytes) && !has(&wal));
    }
}
