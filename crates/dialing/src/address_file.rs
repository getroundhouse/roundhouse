//! Where the hub's loopback endpoint is now, which the hub writes to a file in its data directory each time it binds
//! that endpoint, apart from the file that holds its secret key. A local worker reads it once a dial has failed, so it
//! finds a hub that came back on another port, as the owner decided on 2026-11-27: other hubs on the machine draw from
//! the same ports, and a local worker has no relay to fall back on. The file says where the hub is, not who it is, so a
//! worker dials what it names only if it's the key its ticket names, as [`WorkerEndpoint::moved_to`] says.
//!
//! [`WorkerEndpoint::moved_to`]: crate::WorkerEndpoint::moved_to

use std::path::Path;

use protocol::dialing::HubAddress;

use crate::DialError;

/// Writes `address` to `path`, whole, readable only by this user, as every local worker runs as the hub's own user.
pub fn write(path: &Path, address: &HubAddress) -> Result<(), DialError> {
    let failed = |why: String| DialError::AddressFile { path: path.to_path_buf(), why };
    let json = serde_json::to_vec(address).map_err(|e| failed(e.to_string()))?;
    crate::private_file::write(path, &json).map_err(|e| failed(e.to_string()))
}

/// What the hub last wrote to `path`.
pub fn read(path: &Path) -> Result<HubAddress, DialError> {
    let failed = |why: String| DialError::AddressFile { path: path.to_path_buf(), why };
    let json = std::fs::read(path).map_err(|e| failed(e.to_string()))?;
    serde_json::from_slice(&json).map_err(|e| failed(e.to_string()))
}
