//! What the tests share: a real hub with the fake agent, and the app in a phone-sized window, driven the way someone
//! would. The app pairs and connects as it does on a phone, over TLS, to a hub listening on loopback.

#![cfg(test)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use client_core::Client;
use client_core::link::Waits;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, SharedString, TestAppContext, WindowBounds,
    WindowOptions, point, px, size,
};
use mobile::{MemoryVault, MobileApp, Vault as _};
use process::testing::WholeEnvironment;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Repo, Seq, Workspace};
use protocol::pairing::PairingLink;
use protocol::paths::{ENV_HOME, ENV_LISTEN, ENV_WORKSPACES, Paths};
use tempfile::TempDir;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);
/// How far the app's own clock moves on each look while a test waits on a real hub.
const STEP: Duration = Duration::from_millis(10);
/// How long the phone waits before trying a hub again, first and longest, in place of the second and the thirty
/// seconds every client waits.
const RETRY_WAITS: Waits = Waits::retrying(Duration::from_millis(20), Duration::from_millis(100));

/// The ID of an element named `name`, as the question panel names its own.
pub(crate) fn id(name: &str) -> SharedString {
    SharedString::from(name.to_string())
}

/// The hub and the fake agent live in other packages, so `cargo test -p mobile-ios` wouldn't otherwise build them.
fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
        process::testing::the_hub_and_examples(tmp, "phone-binaries", &["fake_claude"])
    })
}

/// A hub of its own, with a repository, which stops with the test. It keeps one port through restarts, as a hub people
/// use does, so a paired phone finds it again.
pub(crate) struct Stack {
    dir: TempDir,
    paths: Paths,
    port: u16,
    hub: Option<Child>,
}

impl Stack {
    pub(crate) fn new() -> Stack {
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("phui-").tempdir_in("/tmp").unwrap();
        let root = dir.path().to_path_buf();
        process::testing::repository_with_a_commit(&root.join("repo"));
        let paths = protocol::paths::resolve(|name| match name {
            ENV_HOME => Some(root.join("home").to_string_lossy().into_owned()),
            ENV_WORKSPACES => Some(root.join("workspaces").to_string_lossy().into_owned()),
            other => std::env::var(other).ok(),
        })
        .unwrap();
        // Free when asked. Another test could take it before the hub does, which would fail that test loudly.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut stack = Stack { dir, paths, port, hub: None };
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

    pub(crate) fn start_hub(&mut self) {
        let hub = environment(self.dir.path())
            .set(ENV_HOME, &self.paths.data_dir)
            .set(ENV_WORKSPACES, &self.paths.workspaces_root)
            // Only on loopback, so no firewall asks.
            .set(ENV_LISTEN, format!("127.0.0.1:{}", self.port))
            .command(binaries().join("roundhoused"))
            .arg("hub")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.hub = Some(hub);
        let socket = &self.paths.socket;
        waiting::thread::until(TIMEOUT, "timed out waiting for the hub to start", || {
            Client::connect(socket, "test").is_ok()
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
    pub(crate) fn client(&self) -> Client {
        Client::connect(&self.paths.socket, "test").unwrap()
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
        .set("ROUNDHOUSE_CLAUDE_BIN", binaries().join("examples/fake_claude"))
        .set("FAKE_CLAUDE_STATE", root.join("agent-state"))
        // So a test waits out only the time limits it's about. The hub passes the workers' on to them.
        .set(timers, short)
        .set(worker_timers, worker_short)
        // So an agent started again doesn't sit through the CLI's 2 s before it asks after a task.
        .set("FAKE_CLAUDE_POLL_DEFAULT_MS", worker::Timers::FOR_TESTS.wait_poll.as_millis().to_string());
    environment
}

pub(crate) struct Phone<'a> {
    pub(crate) cx: &'a mut TestAppContext,
    pub(crate) handle: AnyWindowHandle,
    pub(crate) app: Entity<MobileApp>,
    pub(crate) vault: Arc<MemoryVault>,
    /// Pairing links, as the Camera app opens them.
    pub(crate) links: flume::Sender<String>,
    /// The approval [`Phone::next_open_approval`] last returned.
    seen_approval: Seq,
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
        // Tried again by the system's clock, which a test can't move, so it would sit through the phone's waits.
        cx.update(|cx| mobile::the_retry_waits_are(RETRY_WAITS, cx));
        let (links, opened) = flume::unbounded();
        // An iPhone's screen, in points.
        let screen = Bounds::new(point(px(0.), px(0.)), size(px(402.), px(874.)));
        let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(screen)), ..Default::default() };
        let (handle, app) = cx.update(|cx| mobile::open_window(vault.clone(), opened, options, cx)).unwrap();
        Phone { cx, handle, app, vault, links, seen_approval: Seq::default() }
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

    /// Runs the app until `done`, on real time, because a real hub is on the other end of the socket. The app's own
    /// timers run on the test's clock, so that moves on with it. The wait before connecting again is client-core's, on
    /// the system's clock, which [`RETRY_WAITS`] shortens.
    pub(crate) fn wait(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        waiting::thread::until(TIMEOUT, format!("timed out waiting for {what}"), || {
            self.frame();
            let done = done(self);
            self.cx.executor().advance_clock(STEP);
            done
        });
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

    /// Types where the cursor is, as the on-screen keyboard does.
    pub(crate) fn type_text(&mut self, text: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
        self.frame();
    }

    /// Taps the field, then types into it.
    pub(crate) fn type_into(&mut self, field: &str, text: &str) {
        self.tap(id(field));
        self.type_text(text);
    }

    /// Taps the workspace in the list, once it's there and the phone is connected, and waits for its chat. The list
    /// draws the records client-core holds, which can be there before the connection is ready, or after it went, and a
    /// tap opens nothing without a connection.
    pub(crate) fn open_chat(&mut self, workspace: &Workspace) {
        let row = ElementId::Name(format!("workspace-{}", workspace.id).into());
        self.wait("the workspace", |phone| phone.has(row.clone()) && phone.is_connected());
        self.tap(row);
        self.wait("its chat", |phone| phone.has("composer") && phone.transcript_shown());
    }

    /// Types into the chat's composer and sends it.
    pub(crate) fn send(&mut self, text: &str) {
        self.type_into("composer", text);
        self.tap("send");
    }

    /// The newest open approval, once the transcript has it, and it's newer than the last this returned. Its status
    /// and its events come apart, so either can arrive first.
    pub(crate) fn next_open_approval(&mut self) -> Seq {
        let seen = self.seen_approval;
        let newest = |phone: &Self| {
            let transcript = phone.cx.read_entity(&phone.app, MobileApp::transcript)?;
            let approvals = phone.cx.read_entity(&transcript, |t, _| t.open_approvals());
            approvals.iter().map(|event| event.seq).filter(|seq| *seq > seen).max()
        };
        self.wait("the transcript to show the approval", |phone| newest(phone).is_some());
        self.seen_approval = newest(self).unwrap();
        self.seen_approval
    }

    pub(crate) fn chat_title(&self) -> Option<String> {
        self.cx.read_entity(&self.app, MobileApp::chat_title)
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
                    if let ContentBlock::Text { text: t, .. } = block {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    /// What the agent was told its calls came to, such as the answer to its question.
    pub(crate) fn told_to_agent(&self) -> String {
        let mut told = String::new();
        for event in self.events() {
            if let EventBody::AgentUser { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::CallResult { output, .. } = block {
                        told.push_str(&client_core::calls::typed::output(output));
                    }
                }
            }
        }
        told
    }
}
