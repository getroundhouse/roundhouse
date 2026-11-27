//! The hub the tests talk to: one of its own for each test, with a repository, and the fake agent in the place of the
//! real one.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use client_core::Connection;
use process::testing::WholeEnvironment;
use protocol::model::{Repo, Workspace};
use protocol::paths::{ENV_HOME, ENV_LISTEN, ENV_WORKSPACES, Paths};
use tempfile::TempDir;

use super::phone::TIMEOUT;

/// The hub and the fake agent live in other packages, so `cargo test -p mobile-ios` wouldn't otherwise build them.
fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
        process::testing::the_examples(tmp, "phone-binaries", &["daemon_for_tests", "fake_claude"])
    })
}

/// A hub of its own, with a repository, which stops with the test. It keeps one port through restarts, as a hub people
/// use does, so a paired phone finds it again.
pub(crate) struct Stack {
    dir: TempDir,
    paths: Paths,
    /// The port the hub chose as it first listened for devices, for a pairing link. A free port asked for and let go of
    /// before the hub took it could be given to another test's hub too, which could then take it while this one restarts.
    port: OnceCell<u16>,
    hub: Option<Child>,
}

impl Stack {
    pub(crate) fn new() -> Stack {
        let dir = process::testing::tmp_dir("phui-");
        let root = dir.path().to_path_buf();
        process::testing::repository_with_a_commit(&root.join("repo"));
        let paths = protocol::paths::resolve(|name| match name {
            ENV_HOME => Some(root.join("home").to_string_lossy().into_owned()),
            ENV_WORKSPACES => Some(root.join("workspaces").to_string_lossy().into_owned()),
            other => std::env::var(other).ok(),
        })
        .unwrap();
        let mut stack = Stack { dir, paths, port: OnceCell::new(), hub: None };
        stack.start_hub();
        stack
    }

    /// The stack's repository, added to the hub.
    pub(crate) fn add_repo(&self) -> Repo {
        self.client().repo_add(self.dir.path().join("repo").to_str().unwrap()).unwrap()
    }

    /// A file in the stack's directory that the fake agent's `hang-until:` waits for, until it's made.
    pub(crate) fn gate(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A workspace in the stack's repository, made by another client, as from the desktop app.
    pub(crate) fn workspace(&self) -> Workspace {
        self.client().workspace_create(&self.add_repo().id, None, None, None).unwrap()
    }

    /// Starts the hub on the port it chose before, if it has, and otherwise lets it choose one as it first listens.
    pub(crate) fn start_hub(&mut self) {
        let port = self.port.get().copied().unwrap_or(0);
        let hub = environment(self.dir.path())
            .set(ENV_HOME, &self.paths.data_dir)
            .set(ENV_WORKSPACES, &self.paths.workspaces_root)
            // Only on loopback, so no firewall asks.
            .set(ENV_LISTEN, format!("127.0.0.1:{port}"))
            .command(binaries().join("daemon_for_tests"))
            .arg("hub")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.hub = Some(hub);
        let socket = &self.paths.socket;
        waiting::thread::until(TIMEOUT, "timed out waiting for the hub to start", || {
            Connection::connect(socket, "test").is_ok()
        });
    }

    /// Stops the hub as the app does, and waits for it to exit.
    pub(crate) fn stop_hub(&mut self) {
        let Some(mut hub) = self.hub.take() else { return };
        let stopped = client_core::stop_hub(&self.paths.socket);
        // Not again in a test that failed, since a panic as it unwinds would end the run without saying why.
        assert!(stopped.is_ok() || std::thread::panicking(), "the hub didn't stop: {stopped:?}");
        waiting::thread::up_to(Duration::from_secs(5), || !matches!(hub.try_wait(), Ok(None)));
        let _ = hub.kill();
        let _ = hub.wait();
    }

    /// A client on the hub's own machine, for setting things up and checking them from outside the app.
    pub(crate) fn client(&self) -> Connection {
        Connection::connect(&self.paths.socket, "test").unwrap()
    }

    /// What the hub's QR code holds now, which points only at loopback, where the hub listens, on the port it chose the
    /// first time.
    pub(crate) fn pairing_link(&self) -> String {
        let link = self.client().pairing_start().unwrap().link;
        let [host] = link.hosts.as_slice() else { panic!("the link offers {:?}", link.hosts) };
        let port = host.strip_prefix("127.0.0.1:").and_then(|port| port.parse().ok());
        let port = port.unwrap_or_else(|| panic!("the link offers {host}, which isn't on loopback"));
        assert_eq!(*self.port.get_or_init(|| port), port, "the hub listens on another port than it chose");
        link.to_string()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        self.stop_hub();
    }
}

/// The whole environment the hub runs in, as the desktop's tests start theirs: what [`WholeEnvironment::for_a_test`]
/// gives, with a home of its own under `root`, so a real ~/.conductor/settings.toml or `gh` login can't change what
/// these tests do, a `gh` that's logged in to nothing, the fake agent, and the timers that only make a test wait
/// shortened.
fn environment(root: &Path) -> WholeEnvironment {
    let (timers, short) = hub::Timers::FOR_TESTS.env();
    let (worker_timers, worker_short) = worker::Timers::FOR_TESTS.env();
    let mut environment = WholeEnvironment::for_a_test(&root.join("user-home"));
    environment
        .put_first_on_path(process::testing::logged_out_gh())
        .set("ROUNDHOUSE_CLAUDE_BIN", binaries().join("fake_claude"))
        .set("FAKE_CLAUDE_STATE", root.join("agent-state"))
        // So a test waits out only the time limits it's about. The hub passes the workers' on to them.
        .set(timers, short)
        .set(worker_timers, worker_short)
        // So an agent started again doesn't sit through the CLI's 2 s before it asks after a task.
        .set("FAKE_CLAUDE_POLL_DEFAULT_MS", worker::Timers::FOR_TESTS.wait_poll.as_millis().to_string());
    environment
}
