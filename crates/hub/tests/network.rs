//! Real loopback TLS, the same pairing and client transport used by the desktop.

#![cfg(test)]

use std::time::{Duration, Instant};

use client_core::{ClientError, Connection};
use protocol::client::ErrorCode;
use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaveCancel};
use protocol::client::tool_servers::ToolServerAdd;
use protocol::model::ToolServerAuthGiven;
use protocol::pairing::Fingerprint;

struct Running {
    local: Connection,
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
            relays: hub::Relays::None,
            worker_exe: dir.path().join("unused-worker"),
            paths,
            shell: None,
            home: None,
            path: None,
            start: protocol::client::HubStart::ByHand,
            update_feed: None,
            agents: Vec::new(),
            timers: hub::Timers::DEFAULT,
            github: hub::GithubConfig::at("http://127.0.0.1:9".into()),
        };
        let task = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            // It runs in the test's own process, so it goes with it.
            runtime.block_on(hub::run(config, std::future::pending())).unwrap();
            runtime.shutdown_timeout(Duration::from_secs(1));
        });
        let local = waiting::thread::until_ok(Duration::from_secs(10), || {
            Connection::connect(&socket, "network-test").map_err(|error| format!("hub did not start: {error}"))
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
    let client = Connection::connect_remote(&remote, "reopened desktop").unwrap();
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
    let remote = Connection::connect_remote(&hub, "desktop").unwrap();

    assert_eq!(remote.repo_list().unwrap(), running.local.repo_list().unwrap());
    assert_eq!(code(remote.hub_shutdown().unwrap_err()), ErrorCode::LocalOnly);
    assert_eq!(code(remote.device_list().unwrap_err()), ErrorCode::LocalOnly);
    assert_eq!(code(remote.pairing_start().unwrap_err()), ErrorCode::LocalOnly);
    // A paired device may update a hub installed as a service, which this one isn't, so it's past what's local only.
    let update = remote.call(protocol::client::admin::HubUpdateTo { version: "0.1.9".into() }).unwrap_err();
    assert_eq!(code(update), ErrorCode::InvalidRequest);
    // So are the hub's MCP servers, since a desktop on another machine adds and edits them: each past what's local
    // only, to a refusal of what it asks.
    let add =
        ToolServerAdd { id: "t".into(), name: String::new(), url: String::new(), auth: ToolServerAuthGiven::Neither };
    let save = ToolServerSave { id: "s".into(), of: ToolServerToCheck::Unknown, when: SaveWhen::Listed };
    assert_eq!(code(remote.call(add).unwrap_err()), ErrorCode::InvalidRequest);
    assert_eq!(code(remote.call(save).unwrap_err()), ErrorCode::InvalidRequest);
    remote.call(ToolServerSaveCancel { id: "s".into() }).unwrap();
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
    assert!(Connection::connect_remote(&hub, "revoked").is_err());
    assert!(running.local.device_list().unwrap().is_empty());
    running.stop();
}

#[test]
fn an_unpaired_connection_and_a_different_server_key_are_refused() {
    let running = Running::start();
    let pairing = running.local.pairing_start().unwrap();
    let unpaired = Connection::open_remote(&pairing.link.hosts, &pairing.link.fingerprint).unwrap();
    assert_eq!(code(unpaired.records().unwrap_err()), ErrorCode::Unauthorized);

    let wrong = Fingerprint::of_sha256([0; 32]);
    let result = Connection::open_remote(&pairing.link.hosts, &wrong);
    assert!(matches!(result, Err(ClientError::FingerprintMismatch { .. })));
    assert!(running.local.device_list().unwrap().is_empty());
    running.stop();
}

/// How a test connects to a paired hub, as an app does, with what pairing gave it, which pairing again renews.
struct Paired(client_core::RemoteHub);

impl client_core::link::Connect for Paired {
    type With = ();
    type Failure = String;

    fn connect(&mut self) -> Result<(Connection, ()), client_core::link::Refused<String>> {
        Connection::connect_remote(&self.0, "test")
            .map(|client| (client, ()))
            .map_err(|e| client_core::link::Refused::by(&e, e.to_string()))
    }

    fn lost(&self, lost: &client_core::link::Lost) -> String {
        lost.to_string()
    }
}

#[test]
fn pending_drafts_resume_after_the_same_hub_is_paired_again() {
    use client_core::link::{Link, Waits};
    use client_core::{DraftChange, DraftKey};
    use protocol::model::NewWorkspaceDraftToKeep;

    let running = Running::start();
    let pairing = running.local.pairing_start().unwrap();
    let first = client_core::pair(&pairing.link, "Desktop").unwrap();
    let waits = Waits::retrying(Duration::from_millis(20), Duration::from_millis(100));
    let link = Link::start_waiting(Paired(first.clone()), waits);
    let hub = link.hub();
    waiting::thread::until(Duration::from_secs(10), "never caught up", || hub.caught_up());
    let drafts = hub.drafts();
    running.local.device_revoke(&first.device_id).unwrap();
    waiting::thread::until(Duration::from_secs(10), "the connection never went", || !hub.caught_up());
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
    link.connect_with(Paired(second));
    waiting::thread::until(Duration::from_secs(10), "never caught up again", || hub.caught_up());
    drafts.flushed().recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(!drafts.unkept(&DraftKey::NewWorkspace));
    assert_eq!(running.local.records().unwrap().new_workspace_draft.unwrap().text, "Kept across re-pairing");
    running.stop();
}
