//! Where the app finds its hub.

use std::path::PathBuf;

use client_core::{Client, ClientError};
use gpui_kit::Result;
use protocol::paths::Paths;

/// Where to find the hub, and how to start one if none is running.
#[derive(Clone, Debug)]
pub struct HubSource {
    pub paths: Paths,
    pub daemon: PathBuf,
}

impl HubSource {
    /// The default hub for this user, with the daemon next to this binary.
    pub fn default_local() -> anyhow::Result<Self> {
        let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
        let daemon = std::env::var_os(protocol::paths::ENV_DAEMON_BIN).map_or_else(
            || std::env::current_exe().map_or_else(|_| "trestled".into(), |exe| exe.with_file_name("trestled")),
            PathBuf::from,
        );
        Ok(Self { paths, daemon })
    }

    pub(super) fn connect(&self) -> Result<Client, ClientError> {
        Client::connect_or_start(&self.paths, &self.daemon, "desktop")
    }
}
