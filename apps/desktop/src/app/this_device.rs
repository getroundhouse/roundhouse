//! Which device this app runs on, as it says in its hello to the hub on its own machine.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::Path;

use protocol::model::{ClientDevice, DeviceId};

/// The file in the data directory that keeps this device's ID, which the app makes the first time it connects.
const FILE: &str = "device-id";

/// This device: the ID kept in `data_dir`, made and kept there the first time, and this computer's name. `None` when
/// the ID can't be read or kept, so the app connects as on no device, and the hub refuses what needs one. It reads the
/// computer's name, which on a Mac runs `scutil`, so it's called off the main thread.
pub(super) fn this_device(data_dir: &Path) -> Option<ClientDevice> {
    let id = match kept_id(&data_dir.join(FILE)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("couldn't keep this device's ID in {}, so it connects as on none: {e}", data_dir.display());
            return None;
        }
    };
    Some(ClientDevice { id, name: super::device_name::read_now() })
}

/// The ID kept at `path`, or a new one kept there if there's none. Kept only if no other copy of the app kept one
/// meanwhile, which is then the one.
fn kept_id(path: &Path) -> std::io::Result<DeviceId> {
    if let Some(kept) = read_kept(path)? {
        return Ok(kept);
    }
    let made = client_core::new_device_id();
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(made.as_str().as_bytes())?;
            Ok(made)
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            read_kept(path)?.ok_or_else(|| std::io::Error::other("another copy of the app is keeping it"))
        }
        Err(e) => Err(e),
    }
}

/// The ID kept at `path`, if there's one.
fn read_kept(path: &Path) -> std::io::Result<Option<DeviceId>> {
    match std::fs::read_to_string(path) {
        Ok(kept) => Ok(Some(kept.trim()).filter(|kept| !kept.is_empty()).map(DeviceId::from)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::kept_id;

    #[test]
    fn the_id_is_made_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-id");

        let made = kept_id(&path).unwrap();
        let again = kept_id(&path).unwrap();

        assert!(!made.as_str().is_empty());
        assert_eq!(again, made, "read back, not made again");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), made.as_str());
    }
}
