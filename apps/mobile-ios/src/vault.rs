//! Where the phone keeps the hub it paired with. The token is a credential, so it goes in the Keychain, and the
//! rest, which says only where the hub is, goes in a file beside the app's other data.

use client_core::RemoteHub;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("couldn't save the hub's token in the Keychain: {0}")]
    Keychain(String),
    #[error("couldn't write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Encode(#[from] serde_json::Error),
}

/// One hub. Pairing with another replaces it.
pub trait Vault: Send + Sync {
    /// The hub this phone paired with, or `None` if it hasn't, or what was kept can't be read.
    fn load(&self) -> Option<RemoteHub>;
    fn save(&self, hub: &RemoteHub) -> Result<(), VaultError>;
    fn forget(&self);
}

/// Keeps the hub in memory, for tests.
#[cfg(feature = "testing")]
#[derive(Default)]
pub struct MemoryVault(parking_lot::Mutex<Option<RemoteHub>>);

#[cfg(feature = "testing")]
impl Vault for MemoryVault {
    fn load(&self) -> Option<RemoteHub> {
        self.0.lock().clone()
    }

    fn save(&self, hub: &RemoteHub) -> Result<(), VaultError> {
        *self.0.lock() = Some(hub.clone());
        Ok(())
    }

    fn forget(&self) {
        *self.0.lock() = None;
    }
}

#[cfg(target_os = "ios")]
pub use device::DeviceVault;

#[cfg(target_os = "ios")]
mod device {
    use std::path::{Path, PathBuf};

    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password_options};
    use security_framework::passwords_options::PasswordOptions;

    use super::{Vault, VaultError};
    use client_core::{DeviceToken, RemoteHub};

    /// Where in the Keychain the token is kept. There's one, so pairing again replaces it. Keychain items outlive
    /// the app, so a fixed name also means a reinstall's next pairing replaces a token left from before.
    const SERVICE: &str = "hub-token";
    const ACCOUNT: &str = "hub";

    /// The app's own data directory on the phone, and its Keychain.
    pub struct DeviceVault {
        file: PathBuf,
    }

    impl DeviceVault {
        pub fn new(data_dir: &Path) -> DeviceVault {
            DeviceVault { file: data_dir.join("hub.json") }
        }

        fn write(&self, hub: &RemoteHub) -> Result<(), VaultError> {
            let failed = |source| VaultError::Write { path: self.file.display().to_string(), source };
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir).map_err(failed)?;
            }
            let without_token = RemoteHub { token: DeviceToken::from(""), ..hub.clone() };
            std::fs::write(&self.file, serde_json::to_vec(&without_token)?).map_err(failed)
        }
    }

    impl Vault for DeviceVault {
        fn load(&self) -> Option<RemoteHub> {
            let hub: RemoteHub = serde_json::from_slice(&std::fs::read(&self.file).ok()?).ok()?;
            let token = String::from_utf8(get_generic_password(SERVICE, ACCOUNT).ok()?).ok()?;
            Some(RemoteHub { token: token.into(), ..hub })
        }

        fn save(&self, hub: &RemoteHub) -> Result<(), VaultError> {
            // Gone first, since a new item's access can't be set on one that's already there.
            let _ = delete_generic_password(SERVICE, ACCOUNT);
            let keychain = |e: security_framework::base::Error| VaultError::Keychain(e.to_string());
            // A token is this phone's alone, so it stays out of backups that could restore it onto another.
            let access =
                SecAccessControl::create_with_protection(Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly), 0)
                    .map_err(keychain)?;
            let mut options = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
            options.set_access_control(access);
            set_generic_password_options(hub.token.as_str().as_bytes(), options).map_err(keychain)?;
            self.write(hub)
        }

        fn forget(&self) {
            let _ = delete_generic_password(SERVICE, ACCOUNT);
            let _ = std::fs::remove_file(&self.file);
        }
    }
}
