//! Where in its data directory the hub keeps each kind of file.

use std::path::PathBuf;

use protocol::paths::Paths;

/// Where in its data directory the hub keeps what it keeps outside the database.
pub(crate) struct HubDirs {
    pub(crate) attachments: PathBuf,
    pub(crate) bundles: PathBuf,
    pub(crate) worker_logs: PathBuf,
    /// The cache file, [`crate::cache_file::CacheFile`], which the hub keeps its observations and what it last read from GitHub in.
    pub(crate) cache_file: PathBuf,
    /// The data directory itself, in which the files of each kind of login have a directory of their own, as
    /// `LoginSecret::DIR` says.
    pub(crate) data: PathBuf,
    /// Where a hub installed as a service keeps its builds, beside which an update places the next.
    pub(crate) service_builds: PathBuf,
    pub(crate) network_address: std::net::SocketAddr,
    /// Which relays the endpoint that workers dial uses.
    pub(crate) relays: dialing::Relays,
}

impl HubDirs {
    /// Where paired devices reach the hub, and its certificate and key for them, in the data directory.
    pub(crate) fn remote(&self) -> crate::remote::RemoteConfig {
        crate::remote::RemoteConfig {
            address: self.network_address,
            certificate: self.data.join("hub-cert.der"),
            certificate_key: self.data.join("hub-key.der"),
        }
    }

    /// Where `paths` has each.
    pub(crate) fn of(paths: &Paths) -> Self {
        Self {
            attachments: paths.attachments_dir.clone(),
            bundles: paths.bundles_dir.clone(),
            worker_logs: paths.worker_logs_dir.clone(),
            cache_file: paths.github_cache.clone(),
            data: paths.data_dir.clone(),
            service_builds: paths.service_builds_dir(),
            network_address: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            relays: dialing::Relays::N0,
        }
    }

    /// Each in `dir`, for a test's hub.
    #[cfg(test)]
    pub(crate) fn in_dir(dir: &std::path::Path) -> Self {
        Self {
            attachments: dir.join("attachments"),
            bundles: dir.join("bundles"),
            worker_logs: dir.join("worker-logs"),
            cache_file: dir.join("github-cache.db"),
            data: dir.to_path_buf(),
            service_builds: dir.join("service"),
            network_address: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            relays: dialing::Relays::None,
        }
    }
}
