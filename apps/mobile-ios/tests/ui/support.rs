//! What the tests share: a real hub with the fake agent, and the app in a phone-sized window, driven the way
//! someone would. The app pairs and connects as it does on a phone, over TLS, to a hub listening on loopback.

#![cfg(test)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use client_core::{Client, PairingLink};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, TestAppContext, WindowBounds, WindowOptions, point,
    px, size,
};
use mobile::{MemoryVault, MobileApp, Vault as _};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Repo, Workspace};
use tempfile::TempDir;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);
const STEP: Duration = Duration::from_millis(10);

/// The binaries live in another package, so `cargo test -p mobile-ios` wouldn't otherwise rebuild them. Building
/// an example builds the daemon with its tests' features, so its hub reads the variables that keep it on loopback.
fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let output = Command::new(env!("CARGO"))
            .args(["build", "-p", "daemon", "--bin", "roundhoused", "--example", "fake_claude"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        // target/debug/deps/ui-xxxx
        std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf()
    })
}

/// A hub of its own, with a repository, which stops with the test. It keeps one port through restarts, as a hub
/// people use does, so a paired phone finds it again.
pub(crate) struct Stack {
    dir: TempDir,
    port: u16,
    hub: Option<Child>,
}

impl Stack {
    pub(crate) fn new() -> Stack {
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("mobui-").tempdir_in("/tmp").unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("repo")).unwrap();
        git(&root.join("repo"), &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("repo/a.txt"), "one\n").unwrap();
        git(&root.join("repo"), &["add", "."]);
        git(&root.join("repo"), &["commit", "-qm", "first"]);
        // Free when asked. Another test could take it before the hub does, which would fail that test loudly.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut stack = Stack { dir, port, hub: None };
        stack.start_hub();
        stack
    }

    /// The stack's repository, added to the hub.
    pub(crate) fn add_repo(&self) -> Repo {
        self.client().repo_add(self.dir.path().join("repo").to_str().unwrap()).unwrap()
    }

    /// A workspace in the stack's repository, made by another client, as from the desktop app.
    pub(crate) fn workspace(&self) -> Workspace {
        self.client().workspace_create(&self.add_repo().id, None, None, None).unwrap()
    }

    fn socket(&self) -> PathBuf {
        self.dir.path().join("home/hub.sock")
    }

    pub(crate) fn start_hub(&mut self) {
        let path = |name: &str| self.dir.path().join(name);
        let hub = Command::new(binaries().join("roundhoused"))
            .arg("hub")
            .env(protocol::paths::ENV_HOME, path("home"))
            .env(protocol::paths::ENV_WORKSPACES, path("workspaces"))
            .env(protocol::paths::ENV_CLAUDE_BIN, binaries().join("examples/fake_claude"))
            .env("FAKE_CLAUDE_STATE", path("agent-state"))
            // So a real ~/.conductor/settings.toml can't change what these tests do.
            .env("HOME", path("user-home"))
            // Only on loopback, so no firewall asks.
            .env(protocol::paths::ENV_LISTEN, format!("127.0.0.1:{}", self.port))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.hub = Some(hub);
        let deadline = Instant::now() + TIMEOUT;
        while Client::connect(&self.socket(), "test").is_err() {
            assert!(Instant::now() < deadline, "timed out waiting for the hub to start");
            std::thread::sleep(STEP);
        }
    }

    pub(crate) fn stop_hub(&mut self) {
        let Some(mut hub) = self.hub.take() else { return };
        if let Ok(client) = Client::connect(&self.socket(), "test") {
            let _ = client.hub_shutdown();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while hub.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(STEP);
        }
        let _ = hub.kill();
        let _ = hub.wait();
    }

    /// A client on the hub's own machine, for setting things up and checking them from outside the app.
    pub(crate) fn client(&self) -> Client {
        Client::connect(&self.socket(), "test").unwrap()
    }

    /// What the hub's QR code holds now, which points only at loopback, where the hub listens.
    pub(crate) fn pairing_link(&self) -> String {
        let link = self.client().pairing_start().unwrap().link;
        assert_eq!(link.hosts, [format!("127.0.0.1:{}", self.port)]);
        link.to_string()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        self.stop_hub();
    }
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(crate) struct Phone<'a> {
    pub(crate) cx: &'a mut TestAppContext,
    pub(crate) handle: AnyWindowHandle,
    pub(crate) app: Entity<MobileApp>,
    pub(crate) vault: Arc<MemoryVault>,
    /// Pairing links, as the Camera app opens them.
    pub(crate) links: flume::Sender<String>,
}

impl<'a> Phone<'a> {
    /// The app on a phone that has never paired.
    pub(crate) fn unpaired(cx: &'a mut TestAppContext) -> Phone<'a> {
        Phone::open(cx, Arc::new(MemoryVault::default()))
    }

    /// The app on a phone already paired with the stack's hub.
    pub(crate) fn paired(cx: &'a mut TestAppContext, stack: &Stack) -> Phone<'a> {
        let vault = Arc::new(MemoryVault::default());
        let link = PairingLink::parse(&stack.pairing_link()).unwrap();
        vault.save(&client_core::pair(&link, "phone").unwrap()).unwrap();
        Phone::open(cx, vault)
    }

    fn open(cx: &'a mut TestAppContext, vault: Arc<MemoryVault>) -> Phone<'a> {
        // A real hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (links, opened) = flume::unbounded();
        // An iPhone's screen, in points.
        let screen = Bounds::new(point(px(0.), px(0.)), size(px(402.), px(874.)));
        let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(screen)), ..Default::default() };
        let (handle, app) = cx.update(|cx| mobile::open_window(vault.clone(), opened, options, cx)).unwrap();
        Phone { cx, handle, app, vault, links }
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.cx.read_entity(&self.app, MobileApp::is_connected)
    }

    /// Whether the last frame drew it.
    pub(crate) fn has(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
    }

    /// Runs the app until `done`, on real time, because a real hub is on the other end of the socket. The app's
    /// own timers, such as its wait before connecting again, run on the test's clock, so that moves on with it.
    pub(crate) fn wait(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(STEP);
            self.cx.executor().advance_clock(STEP);
        }
    }

    pub(crate) fn frame(&mut self) {
        self.cx.run_until_parked();
        let _ = self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx));
        self.cx.run_until_parked();
    }

    pub(crate) fn tap(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
        self.frame();
    }

    /// Taps the field, then types into it.
    pub(crate) fn type_into(&mut self, field: &str, text: &str) {
        self.tap(field.to_string());
        self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
        self.frame();
    }

    /// Taps the workspace in the list, once it's there, and waits for its chat.
    pub(crate) fn open_chat(&mut self, workspace: &Workspace) {
        let row = ElementId::Name(format!("workspace-{}", workspace.id).into());
        self.wait("the workspace", |phone| phone.has(row.clone()));
        self.tap(row);
        self.wait("its chat", |phone| phone.has("composer") && phone.transcript_shown());
    }

    /// Types into the chat's composer and sends it.
    pub(crate) fn send(&mut self, text: &str) {
        self.type_into("composer", text);
        self.tap("send");
    }

    /// The newest approval the chat has open, by its place in the chat.
    pub(crate) fn open_approval(&self) -> Option<u64> {
        let transcript = self.cx.read_entity(&self.app, MobileApp::transcript)?;
        let approvals = self.cx.read_entity(&transcript, |t, _| t.open_approvals());
        approvals.iter().map(|event| event.seq).max()
    }

    pub(crate) fn transcript_shown(&self) -> bool {
        self.cx.read_entity(&self.app, MobileApp::transcript).is_some()
    }

    fn events(&self) -> Vec<SessionEvent> {
        let Some(transcript) = self.cx.read_entity(&self.app, MobileApp::transcript) else { return Vec::new() };
        self.cx.read_entity(&transcript, |t, _| (0..t.row_count()).filter_map(|row| t.event_at(row)).collect())
    }

    pub(crate) fn assistant_text(&self) -> String {
        let mut text = String::new();
        for event in self.events() {
            if let EventBody::Assistant { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::Text { text: t } = block {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    /// What the agent was told its tools gave back, such as the answer to its question.
    pub(crate) fn told_to_agent(&self) -> String {
        let mut told = String::new();
        for event in self.events() {
            if let EventBody::AgentUser { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::ToolResult { text, .. } = block {
                        told.push_str(text);
                    }
                }
            }
        }
        told
    }
}
