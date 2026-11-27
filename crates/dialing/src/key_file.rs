//! The hub's endpoint's key, and the port it last bound, kept in the hub's data directory so a worker finds it again
//! after the hub restarts, at the key and the addresses its ticket names.

use std::io::Write as _;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use iroh::SecretKey;
use serde::{Deserialize, Serialize};

use crate::DialError;

/// What the file holds, as JSON, which only this user can read, since the key is what a worker trusts the hub by.
#[derive(Serialize, Deserialize)]
struct Kept {
    /// The secret key's 32 bytes, in unpadded base64url.
    key: String,
    /// The UDP port it last bound, which it binds again. `None` until it has bound one.
    #[serde(default)]
    port: Option<u16>,
}

/// The key at `path`, and the port kept with it, making the key first if there's no file.
pub(crate) fn read_or_make(path: &Path) -> Result<(SecretKey, Option<u16>), DialError> {
    let io = |source| DialError::KeyFile { path: path.to_path_buf(), source };
    match std::fs::read(path) {
        Ok(bytes) => {
            let not_a_key = || DialError::NotAKey { path: path.to_path_buf() };
            let kept: Kept = serde_json::from_slice(&bytes).map_err(|_| not_a_key())?;
            let key: [u8; 32] =
                URL_SAFE_NO_PAD.decode(&kept.key).ok().and_then(|key| key.try_into().ok()).ok_or_else(not_a_key)?;
            Ok((SecretKey::from_bytes(&key), kept.port))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = SecretKey::generate();
            write(path, &key, None)?;
            Ok((key, None))
        }
        Err(e) => Err(io(e)),
    }
}

/// Keeps `key` and `port` at `path`, in place of what was there.
pub(crate) fn write(path: &Path, key: &SecretKey, port: Option<u16>) -> Result<(), DialError> {
    let io = |source| DialError::KeyFile { path: path.to_path_buf(), source };
    let kept = Kept { key: URL_SAFE_NO_PAD.encode(key.to_bytes()), port };
    let json = serde_json::to_vec(&kept).map_err(|e| io(std::io::Error::other(e)))?;
    let partial = path.with_extension("partial");
    let mut file = private_files::file_options().write(true).create(true).truncate(true).open(&partial).map_err(io)?;
    file.write_all(&json).and_then(|()| file.sync_all()).map_err(io)?;
    std::fs::rename(&partial, path).map_err(io)
}
