//! Where the app finds its hub.

use std::path::PathBuf;

use client_core::{Client, ClientError};
use gpui_kit::Result;
use protocol::client::HubStatus;
use protocol::paths::Paths;

/// Where to find the hub, and how to start one if none is running.
#[derive(Clone, Debug)]
pub struct HubSource {
    pub paths: Paths,
    pub daemon: PathBuf,
    /// Which build this app is, for telling whether a hub is older.
    pub build_number: Option<u64>,
    /// Which build this app is, for telling people. A picture of the app sets its own, so it doesn't change
    /// with the version.
    pub build_version: &'static str,
}

/// Whether to replace this hub with this app's own. Only an older one, so two versions can't keep replacing
/// each other's, and only one a client started, and only if nothing would notice: this app is the one client,
/// and its request for the status is the one thing the hub is doing.
fn should_replace(hub: &HubStatus, app_build: Option<u64>) -> bool {
    let older = matches!((hub.build_number, app_build), (Some(hub), Some(app)) if hub < app);
    let idle = hub.running_sessions == 0 && hub.running_setups == 0 && hub.requests <= 1;
    older && hub.on_demand && idle && hub.clients <= 1
}

impl HubSource {
    /// The default hub for this user, with the daemon next to this binary.
    pub fn default_local() -> anyhow::Result<Self> {
        let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
        let daemon = std::env::var_os(protocol::paths::ENV_DAEMON_BIN).map_or_else(
            || std::env::current_exe().and_then(|exe| client_core::daemon_beside(&exe)).unwrap_or("roundhoused".into()),
            PathBuf::from,
        );
        Ok(Self { paths, daemon, build_number: protocol::BUILD_NUMBER, build_version: protocol::BUILD_VERSION })
    }

    /// Connects, starting a hub if there's none and replacing one that [`should_replace`] says to. Only here,
    /// while connecting, and only an idle hub, so nothing running is lost. The panes stay, with what's typed in
    /// them.
    pub(super) fn connect(&self) -> Result<Client, ClientError> {
        let client = self.connect_or_start()?;
        if !should_replace(&client.hub_status()?, self.build_number) {
            return Ok(client);
        }
        drop(client);
        client_core::stop_hub(&self.paths.socket)?;
        self.connect_or_start()
    }

    fn connect_or_start(&self) -> Result<Client, ClientError> {
        Client::connect_or_start(&self.paths, &self.daemon, "desktop")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One this app should replace. Each test changes only what it's about.
    fn older_idle_hub() -> HubStatus {
        HubStatus {
            pid: 1,
            hub_version: "older".to_string(),
            protocol_version: protocol::PROTOCOL_VERSION,
            started_at_ms: 0,
            workers: 0,
            running_sessions: 0,
            clients: 1,
            on_demand: true,
            build_number: Some(1),
            running_setups: 0,
            requests: 1,
        }
    }

    #[test]
    fn an_older_idle_hub_a_client_started_is_replaced() {
        assert!(should_replace(&older_idle_hub(), Some(2)));
    }

    #[test]
    fn idle_workers_do_not_keep_a_hub_running() {
        let hub = HubStatus { workers: 3, ..older_idle_hub() };
        assert!(should_replace(&hub, Some(2)));
    }

    #[test]
    fn a_hub_the_same_age_or_newer_is_left_alone() {
        assert!(!should_replace(&older_idle_hub(), Some(1)));
        assert!(!should_replace(&HubStatus { build_number: Some(3), ..older_idle_hub() }, Some(2)));
    }

    #[test]
    fn a_build_with_no_order_neither_replaces_nor_gets_replaced() {
        assert!(!should_replace(&older_idle_hub(), None));
        assert!(!should_replace(&HubStatus { build_number: None, ..older_idle_hub() }, Some(2)));
    }

    #[test]
    fn a_hub_started_by_hand_is_left_alone() {
        assert!(!should_replace(&HubStatus { on_demand: false, ..older_idle_hub() }, Some(2)));
    }

    #[test]
    fn a_hub_with_anything_running_or_another_client_is_left_alone() {
        assert!(!should_replace(&HubStatus { running_sessions: 1, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { running_setups: 1, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { clients: 2, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { requests: 2, ..older_idle_hub() }, Some(2)));
    }
}
