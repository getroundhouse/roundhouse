//! Real loopback TLS, the same pairing and client transport used by the desktop.

#![cfg(test)]

use std::time::{Duration, Instant};

use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::pairing::Fingerprint;

struct Running {
    local: Client,
    task: std::thread::JoinHandle<()>,
    dir: tempfile::TempDir,
}

impl Running {
    fn start() -> Self {
        Self::start_in(tempfile::tempdir().unwrap(), 0)
    }

    fn start_in(dir: tempfile::TempDir, port: u16) -> Self {
        let paths = protocol::paths::resolve(|name| match name {
            protocol::paths::ENV_HOME => Some(dir.path().join("data").display().to_string()),
            protocol::paths::ENV_WORKSPACES => Some(dir.path().join("workspaces").display().to_string()),
            _ => None,
        })
        .unwrap();
        let socket = paths.socket.clone();
        let config = hub::HubConfig {
            build: protocol::Build::UNSTAMPED,
            network_address: ([127, 0, 0, 1], port).into(),
            worker_exe: dir.path().join("unused-worker"),
            paths,
            shell: None,
            home: None,
            path: None,
            start: protocol::client::HubStart::ByHand,
            update_feed: None,
            agents: Vec::new(),
            timers: hub::Timers::DEFAULT,
            github: hub::GithubConfig::from_env(|_| Some("http://127.0.0.1:9".into())),
        };
        let task = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(hub::run(config)).unwrap();
            runtime.shutdown_timeout(Duration::from_secs(1));
        });
        let local = waiting::thread::until_ok(Duration::from_secs(10), || {
            Client::connect(&socket, "network-test").map_err(|error| format!("hub did not start: {error}"))
        });
        Self { local, task, dir }
    }

    fn stop(self) -> tempfile::TempDir {
        self.local.hub_shutdown().unwrap();
        self.task.join().unwrap();
        self.dir
    }
}

#[test]
fn a_saved_device_and_server_key_survive_a_hub_restart() {
    let running = Running::start();
    let pairing = running.local.pairing_start().unwrap();
    let remote = client_core::pair(&pairing.link, "Remembered desktop").unwrap();
    let address: std::net::SocketAddr = remote.hosts[0].parse().unwrap();
    let directory = running.stop();

    let again = Running::start_in(directory, address.port());
    let client = Client::connect_remote(&remote, "reopened desktop").unwrap();
    assert!(client.records().is_ok());
    assert_eq!(again.local.device_list().unwrap()[0].id, remote.device_id);
    again.stop();
}

fn code(error: ClientError) -> ErrorCode {
    match error {
        ClientError::Api(error) => error.code,
        other => panic!("expected an API error, got {other}"),
    }
}

#[test]
fn paired_clients_read_records_but_cannot_administer_and_revocation_closes_them() {
    let running = Running::start();
    let pairing = running.local.pairing_start().unwrap();
    let hub = client_core::pair(&pairing.link, "Test desktop").unwrap();
    let remote = Client::connect_remote(&hub, "desktop").unwrap();

    assert_eq!(remote.repo_list().unwrap(), running.local.repo_list().unwrap());
    assert_eq!(code(remote.hub_shutdown().unwrap_err()), ErrorCode::LocalOnly);
    assert_eq!(code(remote.device_list().unwrap_err()), ErrorCode::LocalOnly);
    assert_eq!(code(remote.pairing_start().unwrap_err()), ErrorCode::LocalOnly);
    // A paired device may update a hub installed as a service, which this one isn't, so it's past what's local only.
    let update = remote.call(protocol::client::admin::HubUpdateTo { version: "0.1.9".into() }).unwrap_err();
    assert_eq!(code(update), ErrorCode::InvalidRequest);
    assert_eq!(code(client_core::pair(&pairing.link, "Second use").unwrap_err()), ErrorCode::Unauthorized);
    assert_eq!(running.local.pairing_wait(&pairing.code).unwrap().id, hub.device_id);
    // It said no device in its hello, and is taken to be the one it paired as: past the device, to the chat.
    let no_chat = remote.session_conversation_start(&"no-such-chat".into()).unwrap_err();
    assert_eq!(code(no_chat), ErrorCode::NotFound);

    running.local.device_revoke(&hub.device_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while remote.records().is_ok() {
        assert!(Instant::now() < deadline, "revoked connection stayed open");
    }
    assert!(Client::connect_remote(&hub, "revoked").is_err());
    assert!(running.local.device_list().unwrap().is_empty());
    running.stop();
}

#[test]
fn an_unpaired_connection_and_a_different_server_key_are_refused() {
    let running = Running::start();
    let pairing = running.local.pairing_start().unwrap();
    let unpaired = Client::open_remote(&pairing.link.hosts, &pairing.link.fingerprint).unwrap();
    assert_eq!(code(unpaired.records().unwrap_err()), ErrorCode::Unauthorized);

    let wrong = Fingerprint::of_sha256([0; 32]);
    let result = Client::open_remote(&pairing.link.hosts, &wrong);
    assert!(matches!(result, Err(ClientError::FingerprintMismatch { .. })));
    assert!(running.local.device_list().unwrap().is_empty());
    running.stop();
}

#[test]
fn pending_drafts_resume_after_the_same_hub_is_paired_again() {
    use client_core::{DraftChange, DraftKey, Drafts};
    use protocol::model::NewWorkspaceDraftToKeep;

    let running = Running::start();
    let pairing = running.local.pairing_start().unwrap();
    let first = client_core::pair(&pairing.link, "Desktop").unwrap();
    let client = Client::connect_remote(&first, "test").unwrap();
    let drafts = Drafts::start(&client);
    running.local.device_revoke(&first.device_id).unwrap();
    assert!(client.records().is_err());
    drafts.set(DraftChange::NewWorkspace(NewWorkspaceDraftToKeep {
        text: "Kept across re-pairing".into(),
        ..Default::default()
    }));
    drafts.flushed().recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(drafts.unkept(&DraftKey::NewWorkspace));

    let pairing = running.local.pairing_start().unwrap();
    let second = client_core::pair(&pairing.link, "Desktop").unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_ne!(first.device_id, second.device_id);
    drafts.reconnect(&Client::connect_remote(&second, "test").unwrap());
    drafts.flushed().recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(!drafts.unkept(&DraftKey::NewWorkspace));
    assert_eq!(running.local.records().unwrap().new_workspace_draft.unwrap().text, "Kept across re-pairing");
    running.stop();
}
