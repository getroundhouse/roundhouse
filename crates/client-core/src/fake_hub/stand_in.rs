use protocol::client::{CLIENT_PROTOCOL_VERSION, HubStatus};
use protocol::model::{AgentDescription, LoginDescription};
use protocol::pairing::{Pairing, PairingCode, PairingLink};

use super::world::{NOW_MS, World};

/// The build the app and its hub both say they are, so the label on every screen stays the same whatever this
/// checkout's version is.
pub const BUILD: &str = "1.0.0";
/// The app's build number, which a world's hub can be older than.
pub const BUILD_NUMBER: u64 = 2;

/// Claude as the app's own hub describes it, so the picker, the context's cards, a turn's card and Agents say what
/// they would.
pub(super) fn claude() -> AgentDescription {
    AgentDescription {
        login: Some(LoginDescription {
            made_with: "claude setup-token".into(),
            lasts_ms: Some(365 * 24 * 60 * 60 * 1000),
        }),
        ..protocol::fixtures::claude()
    }
}

pub(super) fn version(world: &World) -> &'static str {
    world.hub.map_or(BUILD, |hub| hub.version)
}

/// Started by hand, as far as the app can tell, unless the world says otherwise. A hub a client started is never
/// replaced, because its running chats keep it busy. They're counted as the hub counts them.
pub(super) fn status(world: &World) -> HubStatus {
    let running = world.sessions.iter().filter(|s| s.status.counts_as_running(&s.background_tasks)).count();
    HubStatus {
        pid: std::process::id(),
        hub_version: version(world).to_string(),
        protocol_version: CLIENT_PROTOCOL_VERSION,
        started_at_ms: 0,
        workers: 0,
        running_sessions: u32::try_from(running).unwrap(),
        clients: 1,
        on_demand: world.hub.is_some_and(|hub| hub.on_demand),
        started_by: None,
        build_number: world.hub.map(|hub| hub.number),
        running_setups: 0,
        requests: 1,
        keeping_awake: false,
        keeps_workers: false,
        oldest_protocol_version: None,
        oldest_running_worker_protocol: None,
        open_terminals: u32::try_from(world.terminals.len()).unwrap(),
    }
}

pub(super) fn fixed_pairing() -> Pairing {
    let fingerprint = "q3B0ZWxsZXJzLWZpbmdlcnByaW50LXNoYTI1Ni1oYXN".parse().unwrap();
    let code = PairingCode::from("9f3k7m2p4q8w1x6z");
    Pairing {
        link: PairingLink {
            hub_name: "Ada’s MacBook Pro".to_string(),
            hosts: vec!["Adas-MacBook-Pro.local:7867".to_string(), "100.64.0.4:7867".to_string()],
            fingerprint,
            code: code.clone(),
        },
        code,
        expires_at_ms: NOW_MS + 10 * 60_000,
    }
}
