//! Where the app finds its hub.

use std::path::PathBuf;

use anyhow::Context as _;
use client_core::{Client, ClientError, Daemon};
use gpui_kit::Result;
use protocol::client::HubStatus;
use protocol::paths::Paths;

/// Where to find the hub, and how to start one if none is running.
#[derive(Clone, Debug)]
pub struct HubSource {
    pub paths: Paths,
    pub daemon: Daemon,
    /// Which build this app is, for telling whether a hub is older.
    pub build_number: Option<u64>,
    /// Which build this app is, for telling people. A picture of the app sets its own, so it doesn't change
    /// with the version.
    pub build_version: &'static str,
}

/// Whether this app may restart this hub. Only one a client started, and only an older one, so two versions
/// can't keep replacing each other's. A newer hub may have written what this app's own can't open. Older is by build
/// number, which a build without one never is, or by protocol, which every build has. A hub only takes a client on
/// its own protocol, so that only counts across a real break.
pub(super) fn may_restart(hub: &HubStatus, app_build: Option<u64>) -> bool {
    let older_build = matches!((hub.build_number, app_build), (Some(hub), Some(app)) if hub < app);
    hub.on_demand && (older_build || hub.protocol_version < protocol::PROTOCOL_VERSION)
}

/// Whether to replace this hub with this app's own without asking: one it may restart, and only if nothing would
/// notice. This app is the one client, and its request for the status is the one thing the hub is doing.
fn should_replace(hub: &HubStatus, app_build: Option<u64>) -> bool {
    let idle = hub.running_sessions == 0 && hub.running_setups == 0 && hub.requests <= 1;
    may_restart(hub, app_build) && idle && hub.clients <= 1
}

impl HubSource {
    /// The default hub for this user, with the daemon next to this binary.
    pub fn default_local() -> anyhow::Result<Self> {
        let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
        let named = std::env::var_os(protocol::paths::ENV_DAEMON_BIN).map(PathBuf::from);
        let daemon = Daemon::find(named).context("finding the daemon next to this binary")?;
        Ok(Self { paths, daemon, build_number: protocol::BUILD_NUMBER, build_version: protocol::BUILD_VERSION })
    }

    /// Connects, starting a hub if there's none and replacing one that [`should_replace`] says to, which is only an
    /// idle one, so nothing running is lost. The panes stay, with what's typed in them. Also returns the hub's status
    /// as it connected.
    pub(super) fn connect(&self) -> Result<(Client, HubStatus), ClientError> {
        let mut client = self.connect_or_start()?;
        let mut status = client.hub_status()?;
        if should_replace(&status, self.build_number) {
            drop(client);
            client_core::stop_hub_if_still(&self.paths.socket, status.pid)?;
            client = self.connect_or_start()?;
            status = client.hub_status()?;
        }
        Ok((client, status))
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
    fn a_busy_older_hub_can_still_be_restarted_by_asking() {
        let busy = HubStatus { running_sessions: 2, running_setups: 1, clients: 3, requests: 2, ..older_idle_hub() };
        assert!(!should_replace(&busy, Some(2)));
        assert!(may_restart(&busy, Some(2)));
    }

    #[test]
    fn a_hub_on_an_older_protocol_can_be_restarted_whatever_its_build() {
        let across_a_break = HubStatus { protocol_version: protocol::PROTOCOL_VERSION - 1, ..older_idle_hub() };
        assert!(may_restart(&HubStatus { build_number: None, ..across_a_break.clone() }, None));
        assert!(!may_restart(&HubStatus { on_demand: false, ..across_a_break }, None));
        let newer =
            HubStatus { protocol_version: protocol::PROTOCOL_VERSION + 1, build_number: None, ..older_idle_hub() };
        assert!(!may_restart(&newer, None));
    }

    #[test]
    fn only_an_older_hub_a_client_started_can_be_restarted() {
        assert!(!may_restart(&older_idle_hub(), Some(1)));
        assert!(!may_restart(&HubStatus { build_number: Some(3), ..older_idle_hub() }, Some(2)));
        assert!(!may_restart(&HubStatus { on_demand: false, ..older_idle_hub() }, Some(2)));
        assert!(!may_restart(&older_idle_hub(), None));
        assert!(!may_restart(&HubStatus { build_number: None, ..older_idle_hub() }, Some(2)));
    }

    #[test]
    fn a_hub_with_anything_running_or_another_client_is_left_alone() {
        assert!(!should_replace(&HubStatus { running_sessions: 1, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { running_setups: 1, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { clients: 2, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { requests: 2, ..older_idle_hub() }, Some(2)));
    }
}
