//! Where the app finds its hub.

use std::path::PathBuf;

use anyhow::Context as _;
use client_core::{Client, ClientError, Daemon, IdleStop, Restart};
use gpui_kit::Result;
use protocol::Build;
use protocol::client::HubStatus;
use protocol::paths::Paths;

/// Where to find the hub, and how to start one if none is running.
#[derive(Clone, Debug)]
pub struct HubSource {
    pub paths: Paths,
    /// A paired hub, or this machine when absent.
    pub remote: Option<client_core::RemoteHub>,
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
    hub.on_demand && (older_build || hub.protocol_version < protocol::client::CLIENT_PROTOCOL_VERSION)
}

/// Whether to replace this hub with this app's own without asking: one it may restart, and only if that cuts nothing
/// off. Nothing is running on it, and this app's request for the status is the one thing the hub is doing, whoever
/// else is connected, as [`HubStatus::is_idle`] says.
pub(super) fn should_replace(hub: &HubStatus, app_build: Option<u64>) -> bool {
    may_restart(hub, app_build) && hub.is_idle()
}

/// Whether to restart this hub at once, whatever is running on it: one the app may restart that keeps its workers
/// running for the next hub, so a restart stops none of its agents. It goes once no setup script is running.
pub(super) fn restarts_at_once(hub: &HubStatus, app_build: Option<u64>) -> bool {
    may_restart(hub, app_build) && hub.keeps_workers
}

/// Which workers the app's update has to keep running on this hub, as [`crate::updates::keep_workers`] takes it: those
/// of the oldest worker protocol it's running, if it's a hub the app restarts once it has updated, and whose workers
/// outlive the restart. A hub from before it said is taken to run workers as old as this app's own hub speaks.
pub(super) fn workers_to_keep(hub: &HubStatus) -> Option<u32> {
    (hub.on_demand && hub.keeps_workers)
        .then(|| hub.oldest_running_worker_protocol.unwrap_or(protocol::worker::HUB_SPEAKS.oldest))
}

/// Whether the hub runs workers older than the ones a hub of this build starts, which the update has to keep, so the
/// oldest can still rise.
pub(super) fn runs_older_workers(hub: &HubStatus) -> bool {
    workers_to_keep(hub).is_some_and(|oldest| oldest < protocol::worker::HUB_SPEAKS.newest)
}

impl HubSource {
    /// The default hub for this user, with the daemon next to this binary, for an app that's `build`.
    pub fn default_local(build: Build) -> anyhow::Result<Self> {
        let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
        let named = std::env::var_os(protocol::paths::ENV_DAEMON_BIN).map(PathBuf::from);
        let daemon = Daemon::find(named, build).context("finding the daemon next to this binary")?;
        Ok(Self { paths, remote: None, daemon, build_number: build.number(), build_version: build.version() })
    }

    /// Connects, starting a hub if there's none, restarting one that [`restarts_at_once`] says to, and replacing one
    /// that [`should_replace`] says to, which is only an idle one, so nothing running is lost. The hub has the last
    /// word on whether it's idle, and one that's busy after all is kept. One restarting that's still finishing, as with
    /// a setup script running, is used until it goes, and then the app connects again. The panes stay, with what's
    /// typed in them. Also returns the hub's status as it connected.
    pub(super) fn connect(&self) -> Result<(Client, Option<HubStatus>), ClientError> {
        if let Some(remote) = &self.remote {
            return Client::connect_remote(remote, "desktop").map(|client| (client, None));
        }
        let mut client = self.connect_or_start()?;
        let mut status = client.hub_status()?;
        if restarts_at_once(&status, self.build_number) {
            match client_core::restart_hub(&client)? {
                Restart::Exited => client = self.connect_or_start()?,
                // Not read again, since it can go at any moment.
                Restart::Finishing => return Ok((client, Some(status))),
            }
            status = client.hub_status()?;
        } else if should_replace(&status, self.build_number) {
            match client_core::stop_hub_if_idle(&client, &self.paths.socket, status.pid)? {
                IdleStop::Stopped => client = self.connect_or_start()?,
                IdleStop::Busy => {}
            }
            status = client.hub_status()?;
        }
        Ok((client, Some(status)))
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
            protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION,
            started_at_ms: 0,
            workers: 0,
            running_sessions: 0,
            clients: 1,
            on_demand: true,
            build_number: Some(1),
            running_setups: 0,
            requests: 1,
            keeping_awake: false,
            keeps_workers: false,
            oldest_protocol_version: None,
            oldest_running_worker_protocol: None,
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

    /// Since a restart stops none of its agents, what's running on it doesn't matter.
    #[test]
    fn an_older_hub_that_keeps_its_workers_is_restarted_at_once_however_busy() {
        let busy = HubStatus { running_sessions: 2, running_setups: 1, clients: 3, requests: 2, ..older_idle_hub() };
        assert!(restarts_at_once(&HubStatus { keeps_workers: true, ..busy.clone() }, Some(2)));
        assert!(!restarts_at_once(&busy, Some(2)), "one that stops its workers waits until it's idle");
        let keeps = HubStatus { keeps_workers: true, ..older_idle_hub() };
        assert!(!restarts_at_once(&keeps, Some(1)), "the same build");
        assert!(!restarts_at_once(&HubStatus { on_demand: false, ..keeps }, Some(2)), "started by hand");
    }

    #[test]
    fn an_update_keeps_the_oldest_workers_of_a_hub_it_restarts_with_them_running() {
        let hub = HubStatus { keeps_workers: true, oldest_running_worker_protocol: Some(4), ..older_idle_hub() };

        assert_eq!(workers_to_keep(&hub), Some(4));
        let from_before = HubStatus { oldest_running_worker_protocol: None, ..hub.clone() };
        assert_eq!(workers_to_keep(&from_before), Some(protocol::worker::HUB_SPEAKS.oldest));
        assert_eq!(workers_to_keep(&HubStatus { on_demand: false, ..hub.clone() }), None, "started by hand");
        assert_eq!(workers_to_keep(&HubStatus { keeps_workers: false, ..hub }), None, "its workers stop with it");
    }

    #[test]
    fn the_app_reads_the_status_again_only_while_the_hub_runs_older_workers_to_keep() {
        let newest = protocol::worker::HUB_SPEAKS.newest;
        let hub = HubStatus { keeps_workers: true, oldest_running_worker_protocol: Some(newest), ..older_idle_hub() };

        assert!(!runs_older_workers(&hub), "only its own");
        let older = HubStatus { oldest_running_worker_protocol: Some(newest - 1), ..hub.clone() };
        assert!(runs_older_workers(&older));
        assert!(!runs_older_workers(&HubStatus { keeps_workers: false, ..older }), "none to keep");
    }

    #[test]
    fn a_busy_older_hub_can_still_be_restarted_by_asking() {
        let busy = HubStatus { running_sessions: 2, running_setups: 1, clients: 3, requests: 2, ..older_idle_hub() };
        assert!(!should_replace(&busy, Some(2)));
        assert!(may_restart(&busy, Some(2)));
    }

    #[test]
    fn a_hub_on_an_older_protocol_can_be_restarted_whatever_its_build() {
        let across_a_break =
            HubStatus { protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION - 1, ..older_idle_hub() };
        assert!(may_restart(&HubStatus { build_number: None, ..across_a_break.clone() }, None));
        assert!(!may_restart(&HubStatus { on_demand: false, ..across_a_break }, None));
        let newer = HubStatus {
            protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION + 1,
            build_number: None,
            ..older_idle_hub()
        };
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
    fn a_hub_with_anything_running_or_under_way_is_left_alone() {
        assert!(!should_replace(&HubStatus { running_sessions: 1, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { running_setups: 1, ..older_idle_hub() }, Some(2)));
        assert!(!should_replace(&HubStatus { requests: 2, ..older_idle_hub() }, Some(2)));
    }

    /// One that's only connected, such as the app on another device, connects again to the next hub, so it would
    /// otherwise keep the update waiting for as long as it stays open.
    #[test]
    fn another_client_connected_doesnt_keep_a_hub() {
        assert!(should_replace(&HubStatus { clients: 2, ..older_idle_hub() }, Some(2)));
    }
}
