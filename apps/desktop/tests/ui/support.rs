//! What the tests share: a running stack, and a window to drive.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use client_core::Client;
use desktop::app::{AppView, Connection, HubSource};
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, SharedString, TestAppContext};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{RepoId, SessionId, SessionStatus, WorkspaceId};
use tempfile::TempDir;
use transcript::Transcript;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

/// The binaries live in other packages, so `cargo test -p desktop` wouldn't otherwise rebuild them.
pub(crate) fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let output = Command::new(env!("CARGO"))
            .args([
                "build",
                "-p",
                "daemon",
                "--bin",
                "trestled",
                "--example",
                "fake_claude",
                "--example",
                "stand_in_hub",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        // target/debug/deps/ui-xxxx
        std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf()
    })
}

/// What the daemons these tests start need to run the fake agent. Set once, and never changed, because
/// tests here run in parallel and nothing else should be reading the environment at that moment.
pub(crate) fn point_daemons_at_the_fake_agent() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        // Under Cargo's target directory, so runs can't pile up in the system's. Conversation IDs are
        // unique, so nothing here carries over from one run to the next.
        let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
        // SAFETY: guarded by the OnceLock, and this runs before any daemon or thread here reads it.
        unsafe {
            std::env::set_var("TRESTLE_CLAUDE_BIN", binaries().join("examples/fake_claude"));
            std::env::set_var("FAKE_CLAUDE_STATE", scratch.join("ui-agent-state"));
            // So a real ~/.conductor/settings.toml can't change what these tests do.
            std::env::set_var("HOME", scratch.join("ui-home"));
        }
    });
}

pub(crate) struct Stack {
    pub(crate) dir: TempDir,
    pub(crate) hub: HubSource,
}

/// Goes away with the test, even a failed one, since nothing else would stop it.
pub(crate) struct StandInHub(pub(crate) Child);

impl Drop for StandInHub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Stack {
    pub(crate) fn new() -> Self {
        point_daemons_at_the_fake_agent();
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("ahui-").tempdir_in("/tmp").unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("repo")).unwrap();
        git(&root.join("repo"), &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("repo/a.txt"), "one\n").unwrap();
        git(&root.join("repo"), &["add", "."]);
        git(&root.join("repo"), &["commit", "-qm", "first"]);
        let paths = protocol::paths::resolve(|name| match name {
            protocol::paths::ENV_HOME => Some(root.join("home").to_string_lossy().into_owned()),
            protocol::paths::ENV_WORKSPACES => Some(root.join("workspaces").to_string_lossy().into_owned()),
            other => std::env::var(other).ok(),
        })
        .unwrap();
        let hub = HubSource { paths, daemon: binaries().join("trestled") };
        Self { dir, hub }
    }

    pub(crate) fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    /// A hub across a real break from this build, as `examples/stand_in_hub` describes.
    pub(crate) fn start_stand_in_hub(&self, args: &[&str]) -> StandInHub {
        let hub = Command::new(binaries().join("examples/stand_in_hub"))
            .args(args)
            .env(protocol::paths::ENV_HOME, &self.hub.paths.data_dir)
            .env(protocol::paths::ENV_WORKSPACES, &self.hub.paths.workspaces_root)
            // Keeping the test's output open would hang whatever reads it, if it were ever left running.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let hub = StandInHub(hub);
        let deadline = Instant::now() + TIMEOUT;
        while !Client::connect(&self.hub.paths.socket, "test").err().is_some_and(|e| e.is_version_mismatch()) {
            assert!(Instant::now() < deadline, "timed out waiting for it to refuse a client");
            std::thread::sleep(Duration::from_millis(10));
        }
        hub
    }

    /// A second client, for setting things up and checking them from outside the app.
    pub(crate) fn client(&self) -> Client {
        Client::connect_or_start(&self.hub.paths, &self.hub.daemon, "test").unwrap()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if let Ok(client) = Client::connect(&self.hub.paths.socket, "test") {
            let _ = client.hub_shutdown();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.hub.paths.socket.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
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

pub(crate) struct Ui<'a> {
    pub(crate) cx: &'a mut TestAppContext,
    pub(crate) handle: AnyWindowHandle,
    pub(crate) view: Entity<AppView>,
    /// The approval [`Ui::next_open_approval`] last returned.
    pub(crate) seen_approval: u64,
}

impl<'a> Ui<'a> {
    pub(crate) fn open(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        let mut ui = Self::open_without_waiting(cx, stack);
        ui.wait("the app to connect", |ui| matches!(ui.view().connection, Connection::Ready(_)));
        ui
    }

    pub(crate) fn open_without_waiting(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        // A real hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let hub = stack.hub.clone();
        let (handle, view) = cx.update(|cx| desktop::open_main_window(hub, cx)).unwrap();
        Self { cx, handle, view, seen_approval: 0 }
    }

    /// Whether the last frame drew it.
    pub(crate) fn has(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
    }

    pub(crate) fn view(&self) -> AppViewRef<'_> {
        AppViewRef(
            self.cx.read_entity(&self.view, |view, _| std::ptr::from_ref::<AppView>(view)),
            std::marker::PhantomData,
        )
    }

    /// Runs the app until `done`, on real time, because a real hub is on the other end of the socket.
    /// The newest open approval, once the transcript has it. A session's status comes over the hub's
    /// feed, and the transcript's events over their own subscription, so either can arrive first, and
    /// the transcript can still be showing the previous approval.
    pub(crate) fn next_open_approval(&mut self) -> u64 {
        let seen = self.seen_approval;
        let newest = |ui: &Self| {
            let approvals = ui.transcript().map(|t| ui.cx.read_entity(&t, |t, _| t.open_approvals()));
            approvals.into_iter().flatten().map(|event| event.seq).filter(|seq| *seq > seen).max()
        };
        self.wait("the transcript to show the approval", |ui| newest(ui).is_some());
        self.seen_approval = newest(self).unwrap();
        self.seen_approval
    }

    pub(crate) fn wait(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn frame(&mut self) {
        self.cx.run_until_parked();
        let _ = self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx));
        self.cx.run_until_parked();
    }

    pub(crate) fn click(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
        self.frame();
    }

    pub(crate) fn input(&mut self, text: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
    }

    pub(crate) fn press(&mut self, key: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.press(key, cx)).unwrap();
        self.frame();
    }

    pub(crate) fn pane(&self) -> Option<Entity<WorkspacePane>> {
        let view = self.view();
        view.selected.as_ref().and_then(|id| view.panes.get(id).cloned())
    }

    pub(crate) fn transcript(&self) -> Option<Entity<Transcript>> {
        self.pane().and_then(|pane| self.cx.read_entity(&pane, |p, _| p.transcript()))
    }

    pub(crate) fn events(&self) -> Vec<SessionEvent> {
        let Some(transcript) = self.transcript() else { return Vec::new() };
        self.cx.read_entity(&transcript, |t, _| (0..t.row_count()).filter_map(|row| t.event_at(row)).collect())
    }

    pub(crate) fn assistant_text(&self) -> String {
        let mut text = String::new();
        for event in self.events() {
            if let EventBody::Assistant { blocks, .. } = &event.body {
                for block in blocks {
                    if let protocol::event::ContentBlock::Text { text: t } = block {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    pub(crate) fn session_status(&self) -> Option<SessionStatus> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.status))
    }

    pub(crate) fn wait_idle_with(&mut self, what: &str, mut text: impl FnMut(&str) -> bool) {
        self.wait(what, |ui| ui.session_status() == Some(SessionStatus::Idle) && text(&ui.assistant_text()));
    }

    pub(crate) fn active_chat(&self) -> SessionId {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| p.active.clone().unwrap())
    }

    pub(crate) fn open_chats(&self) -> Vec<SessionId> {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| {
            p.sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.id.clone()).collect()
        })
    }

    pub(crate) fn composer_text(&self) -> String {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, cx| p.composer.read(cx).text().to_string())
    }

    /// Presses the new chat button and waits until that chat is showing.
    pub(crate) fn new_chat(&mut self) -> SessionId {
        let before = self.open_chats().len();
        self.click("new-chat");
        self.wait("the new chat", |ui| ui.open_chats().len() > before && ui.transcript().is_some());
        let newest = self.open_chats().last().unwrap().clone();
        self.wait("the new chat to show", |ui| ui.active_chat() == newest);
        newest
    }

    pub(crate) fn show_chat(&mut self, id: &SessionId) {
        self.click(SharedString::from(format!("chat-{id}")));
        self.wait("the chat to show", |ui| ui.active_chat() == *id);
    }

    /// Opens a picker and clicks the item at this position, the way someone would.
    pub(crate) fn pick(&mut self, picker: &str, position: usize) {
        self.click(SharedString::from(picker.to_string()));
        self.click(ElementId::Integer(position as u64));
    }

    pub(crate) fn chat_mode(&self) -> Option<String> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.permission_mode.clone()))
    }

    pub(crate) fn first_repo_id(&self) -> RepoId {
        self.view().repos[0].id.clone()
    }

    /// Clicks New for the first repository, and waits until the new workspace is on screen.
    pub(crate) fn new_workspace(&mut self) -> WorkspaceId {
        let before = self.view().workspaces.len();
        let id = format!("new-in-{}", self.first_repo_id());
        self.click(SharedString::from(id));
        self.wait("the new workspace", |ui| {
            ui.view().workspaces.len() > before && ui.transcript().is_some() && !ui.view().busy
        });
        self.view().selected.clone().unwrap()
    }

    pub(crate) fn worktree(&self, id: &WorkspaceId) -> PathBuf {
        let view = self.view();
        let workspace = view.workspaces.iter().find(|w| &w.id == id).unwrap();
        PathBuf::from(workspace.path.clone().unwrap())
    }
}

/// Closes the window, so the app isn't there to start a new hub when the test stops this one. A real
/// window does that on purpose, and in a test it would leave a hub running after the test has finished.
impl Drop for Ui<'_> {
    fn drop(&mut self) {
        let _ = self.cx.update_window(self.handle, |_, window, _| window.remove_window());
        self.cx.run_until_parked();
    }
}

/// Reads the view without holding a borrow of the test context across a wait.
pub(crate) struct AppViewRef<'a>(*const AppView, std::marker::PhantomData<&'a ()>);

impl std::ops::Deref for AppViewRef<'_> {
    type Target = AppView;
    fn deref(&self) -> &AppView {
        // SAFETY: the view lives as long as the window, for the whole test, and everything here runs on
        // one thread. These borrows only last for a single read, never across a frame.
        unsafe { &*self.0 }
    }
}

pub(crate) fn commit_settings(stack: &Stack, toml: &str) {
    let dir = stack.repo().join(".conductor");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.toml"), toml).unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "settings"]);
}
