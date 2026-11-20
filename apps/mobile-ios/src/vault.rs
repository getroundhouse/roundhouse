//! Where the phone keeps the hub it paired with. The token is a credential, so it goes in the Keychain, and the
//! rest, which says only where the hub is, goes in a file beside the app's other data.

use client_core::RemoteHub;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// The Keychain refused, while it was `doing` this with the token, such as saving it.
    #[error("couldn't {doing} the hub's token in the Keychain: {source}")]
    Keychain {
        doing: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("couldn't {doing} {path}: {source}")]
    File {
        doing: &'static str,
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
    /// Forgets the hub, so it isn't loaded again. What's already gone counts as forgotten.
    fn forget(&self) -> Result<(), VaultError>;
}

/// Keeps the hub in memory, for tests, which can have it refuse to forget, as the Keychain can.
#[cfg(feature = "testing")]
#[derive(Default)]
pub struct MemoryVault {
    hub: parking_lot::Mutex<Option<RemoteHub>>,
    refuses_to_forget: std::sync::atomic::AtomicBool,
}

#[cfg(feature = "testing")]
impl MemoryVault {
    /// Has [`Vault::forget`] fail from now on, or not.
    pub fn refuse_to_forget(&self, refuses: bool) {
        self.refuses_to_forget.store(refuses, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(feature = "testing")]
impl Vault for MemoryVault {
    fn load(&self) -> Option<RemoteHub> {
        self.hub.lock().clone()
    }

    fn save(&self, hub: &RemoteHub) -> Result<(), VaultError> {
        *self.hub.lock() = Some(hub.clone());
        Ok(())
    }

    fn forget(&self) -> Result<(), VaultError> {
        if self.refuses_to_forget.load(std::sync::atomic::Ordering::Relaxed) {
            let source = "the test refused".into();
            return Err(VaultError::Keychain { doing: "remove", source });
        }
        *self.hub.lock() = None;
        Ok(())
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
    use client_core::RemoteHub;
    use protocol::pairing::DeviceToken;

    /// What the Keychain says when there's no token to remove: `errSecItemNotFound`.
    const NOT_FOUND: i32 = -25300;

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
            let failed = |source| VaultError::File { doing: "write", path: self.file.display().to_string(), source };
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir).map_err(failed)?;
            }
            let without_token = RemoteHub { token: DeviceToken::from(""), ..hub.clone() };
            std::fs::write(&self.file, serde_json::to_vec(&without_token)?).map_err(failed)
        }
    }

    /// What the Keychain said while `doing` something with the token.
    fn keychain(doing: &'static str) -> impl Fn(security_framework::base::Error) -> VaultError + Copy {
        move |source| VaultError::Keychain { doing, source: Box::new(source) }
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
            let keychain = keychain("save");
            // A token is this phone's alone, so it stays out of backups that could restore it onto another.
            let access =
                SecAccessControl::create_with_protection(Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly), 0)
                    .map_err(keychain)?;
            let mut options = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
            options.set_access_control(access);
            set_generic_password_options(hub.token.as_str().as_bytes(), options).map_err(keychain)?;
            self.write(hub)
        }

        /// The token first, since without it nothing is loaded, whatever's left of the file.
        fn forget(&self) -> Result<(), VaultError> {
            match delete_generic_password(SERVICE, ACCOUNT) {
                Err(e) if e.code() != NOT_FOUND => return Err(keychain("remove")(e)),
                Ok(()) | Err(_) => {}
            }
            match std::fs::remove_file(&self.file) {
                Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
                    Err(VaultError::File { doing: "remove", path: self.file.display().to_string(), source })
                }
                Ok(()) | Err(_) => Ok(()),
            }
        }
    }
}
