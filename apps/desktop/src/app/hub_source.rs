//! Where the app finds its hub.

use std::path::PathBuf;

use anyhow::Context as _;
use client_core::link::{Connect, Lost, Refused};
use client_core::{Client, ClientError, HubAdmin, IdleStop, Introduction, Restart};
use gpui_kit::Result;
use local_hub::Daemon;
use protocol::Build;
use protocol::client::HubStatus;
use protocol::pairing::Fingerprint;
use protocol::paths::Paths;

use super::{ConnectionFailure, Unconnected};

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

/// Whether more is running on the hub `now` than when it `said` what restarting it would stop, so restarting it would
/// stop what nobody was asked about: more agents at work, setup scripts or terminals.
pub(super) fn runs_more_than(now: &HubStatus, said: &HubStatus) -> bool {
    now.running_sessions > said.running_sessions
        || now.running_setups > said.running_setups
        || now.open_terminals > said.open_terminals
}

/// Whether to restart this hub at once, whatever is running on it: one the app may restart that keeps its workers
/// running for the next hub, so a restart stops none of its agents. It goes once no setup script is running.
pub(super) fn restarts_at_once(hub: &HubStatus, app_build: Option<u64>) -> bool {
    may_restart(hub, app_build) && hub.keeps_workers
}

/// Which workers the app's update has to keep running on this hub, as [`crate::updates::keep`] takes it: those
/// of the oldest worker protocol it's running, if it's a hub the app restarts once it has updated, and whose workers
/// outlive the restart. A hub from before it said is taken to run workers as old as this app's own hub speaks.
/// *Unconfirmed:* how old they're taken to be.
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
    /// Which paired hub it is, or `None` for this machine's.
    pub(crate) fn fingerprint(&self) -> Option<&Fingerprint> {
        self.remote.as_ref().map(|hub| &hub.fingerprint)
    }

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
    fn connect_or_replace(&self) -> Result<(Client, Option<HubStatus>), ClientError> {
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
            match client_core::stop_hub_if_idle(&client)? {
                IdleStop::Stopped => client = self.connect_or_start()?,
                IdleStop::Busy => {}
            }
            status = client.hub_status()?;
        }
        Ok((client, Some(status)))
    }

    /// Says in its hello which device this is, so the hub knows which has a chat's conversation.
    fn connect_or_start(&self) -> Result<Client, ClientError> {
        let introduction = match super::this_device::this_device(&self.paths.data_dir) {
            Some(device) => Introduction::on("desktop", device),
            None => Introduction::from("desktop"),
        };
        local_hub::connect_or_start(&self.paths, &self.daemon, introduction)
    }
}

/// The link connects with this. A failure of this machine's hub waits for a person, since only one can start a hub
/// that failed to start, and one across a protocol break says which hub is running, so the app can offer to restart it.
impl Connect for HubSource {
    type With = Option<HubStatus>;
    type Failure = Unconnected;

    fn connect(&mut self) -> Result<(Client, Option<HubStatus>), Refused<Unconnected>> {
        let local = self.remote.is_none();
        self.connect_or_replace().map_err(|e| {
            if !local {
                return Refused::by(&e, Unconnected::Failed(ConnectionFailure::of(false, &e)));
            }
            let across = e.is_version_mismatch().then(|| HubAdmin::connect(&self.paths.socket)?.status());
            match across {
                Some(Ok(status)) => Refused::for_a_person(Unconnected::AcrossABreak(status)),
                Some(Err(_)) | None => Refused::for_a_person(Unconnected::Failed(ConnectionFailure::of(true, &e))),
            }
        })
    }

    fn lost(&self, lost: &Lost) -> Unconnected {
        Unconnected::Failed(ConnectionFailure::of_follow(self.remote.is_none(), lost.to_string()))
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
            started_by: None,
            build_number: Some(1),
            running_setups: 0,
            requests: 1,
            keeping_awake: false,
            keeps_workers: false,
            oldest_protocol_version: None,
            oldest_running_worker_protocol: None,
            open_terminals: 0,
        }
    }

    /// Each of what restarting it stops counts, and less of any doesn't.
    #[test]
    fn more_agents_setups_or_terminals_than_were_said_is_more_running() {
        let said = HubStatus { running_sessions: 1, running_setups: 1, open_terminals: 1, ..older_idle_hub() };

        assert!(runs_more_than(&HubStatus { running_sessions: 2, ..said.clone() }, &said));
        assert!(runs_more_than(&HubStatus { running_setups: 2, ..said.clone() }, &said));
        assert!(runs_more_than(&HubStatus { open_terminals: 2, ..said.clone() }, &said));
        assert!(!runs_more_than(&said, &said));
        assert!(!runs_more_than(&HubStatus { running_sessions: 0, open_terminals: 0, ..said.clone() }, &said));
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

    /// Replaced without asking, as an older hub a client started is while that cuts nothing off.
    const REPLACED: [bool; 3] = [true, true, false];
    /// Restarted only once someone asks, as a busy one is.
    const ASKED: [bool; 3] = [true, false, false];
    /// Restarted at once, as a busy one that keeps its workers is.
    const AT_ONCE: [bool; 3] = [true, false, true];
    /// Left alone.
    const LEFT: [bool; 3] = [false; 3];

    /// What the app does with each hub, for an app of the build given, as `[may_restart, should_replace,
    /// restarts_at_once]` say: restart it if asked, replace it without asking, or restart it at once.
    fn restarts() -> Vec<(&'static str, HubStatus, Option<u64>, [bool; 3])> {
        let idle = older_idle_hub;
        let busy = HubStatus { running_sessions: 2, running_setups: 1, clients: 3, requests: 2, ..idle() };
        let keeps = HubStatus { keeps_workers: true, ..idle() };
        let older_protocol = HubStatus { protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION - 1, ..idle() };
        let newer_protocol = HubStatus { protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION + 1, ..idle() };
        vec![
            ("an older idle hub a client started", idle(), Some(2), REPLACED),
            ("with idle workers", HubStatus { workers: 3, ..idle() }, Some(2), REPLACED),
            // One that's only connected, such as the app on another device, connects again to the next hub, so it
            // would otherwise keep the update waiting for as long as it stays open.
            ("with another client connected", HubStatus { clients: 2, ..idle() }, Some(2), REPLACED),
            ("the same build", idle(), Some(1), LEFT),
            ("a newer build", HubStatus { build_number: Some(3), ..idle() }, Some(2), LEFT),
            ("an app with no build number", idle(), None, LEFT),
            ("a hub with no build number", HubStatus { build_number: None, ..idle() }, Some(2), LEFT),
            ("started by hand", HubStatus { on_demand: false, ..idle() }, Some(2), LEFT),
            ("an agent running", HubStatus { running_sessions: 1, ..idle() }, Some(2), ASKED),
            ("a setup running", HubStatus { running_setups: 1, ..idle() }, Some(2), ASKED),
            // Replacing it would end its shell.
            ("a terminal open", HubStatus { open_terminals: 1, ..idle() }, Some(2), ASKED),
            ("a request under way", HubStatus { requests: 2, ..idle() }, Some(2), ASKED),
            // One that stops its workers waits until it's idle.
            ("busy", busy.clone(), Some(2), ASKED),
            // Since a restart stops none of its agents, what's running on it doesn't matter.
            ("busy, keeping its workers", HubStatus { keeps_workers: true, ..busy }, Some(2), AT_ONCE),
            ("keeping its workers, the same build", keeps.clone(), Some(1), LEFT),
            ("keeping its workers, started by hand", HubStatus { on_demand: false, ..keeps }, Some(2), LEFT),
            // Older by protocol, whatever its build.
            ("an older protocol", HubStatus { build_number: None, ..older_protocol.clone() }, None, REPLACED),
            ("an older protocol, started by hand", HubStatus { on_demand: false, ..older_protocol }, None, LEFT),
            ("a newer protocol", HubStatus { build_number: None, ..newer_protocol }, None, LEFT),
        ]
    }

    /// The app restarts only an older hub a client started, replaces one without asking only while that cuts nothing
    /// off, and restarts one at once only if it keeps its workers.
    #[test]
    fn the_app_restarts_only_an_older_hub_a_client_started_and_without_asking_only_one_that_loses_nothing() {
        for (name, hub, app, want) in restarts() {
            let got = [may_restart(&hub, app), should_replace(&hub, app), restarts_at_once(&hub, app)];

            assert_eq!(got, want, "{name}: [may restart, replaced, restarted at once]");
        }
    }
}
