#![cfg(test)]

use super::daemons;
use client_core::{Client, Daemon};
use daemons::{binaries, point_daemons_at_the_fake_agent};
use desktop::app::{AppView, BuildLine, Connection, HubSource};
use desktop::checkout::Checkout;
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, InputEvent as _, MouseMoveEvent, Pixels, ScrollDelta,
    ScrollEvent, SharedString, TestAppContext,
};
use protocol::model::{Effort, ModelName, PermissionMode, Seq};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use transcript::Transcript;

/// What the hub's Claude gives a chat that names neither, as its adapter describes it.
pub(crate) const CLAUDE_STARTS_WITH: (ModelName, Effort) = (ModelName::named("opus"), Effort::Xhigh);

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Stack {
    pub(crate) dir: TempDir,
    pub(crate) hub: HubSource,
    /// What the app is told it was built from. None, as for a stamped build, unless a test is about it.
    pub(crate) checkout: Option<Checkout>,
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
        let hub = HubSource {
            paths,
            daemon: Daemon::in_place(binaries().join("roundhoused")),
            build_number: None,
            // The daemon's, which was compiled with the same stamp as this test.
            build_version: protocol::this_build!().version(),
        };
        Self { dir, hub, checkout: None }
    }

    pub(crate) fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    /// A second client, for setting things up and checking them from outside the app.
    pub(crate) fn client(&self) -> Client {
        Client::connect_or_start(&self.hub.paths, &self.hub.daemon, "test").unwrap()
    }

    /// Where the app keeps its settings.
    pub(crate) fn settings_file(&self) -> PathBuf {
        self.hub.paths.data_dir.join("settings.json")
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

/// Waits for something outside the app, such as a hub before the app has opened. [`Ui::wait`] is for what the app
/// has to draw first.
pub(crate) fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
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
    pub(crate) seen_approval: Seq,
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
        // A test's clock stands still, so anything that moves would stay where it starts: a sliding sidebar, a mark,
        // the kit's own animations. `motion.rs` has how a test of something that moves plays it.
        cx.update(|cx| cx.set_reduce_motion(true));
        let (hub, checkout) = (stack.hub.clone(), stack.checkout.clone());
        let (handle, view) =
            cx.update(|cx| desktop::open_main_window(hub, checkout, desktop::Opens::WhereItWasLeft, cx)).unwrap();
        Self { cx, handle, view, seen_approval: Seq::default() }
    }

    /// What the line at the bottom of the sidebar says.
    pub(crate) fn build_line(&mut self) -> BuildLine {
        let view = self.view.clone();
        self.cx.update(|cx| view.read(cx).build_line(cx))
    }

    /// Whether the last frame drew it.
    pub(crate) fn has(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
    }

    /// Where the last frame drew it.
    pub(crate) fn bounds(&mut self, id: impl Into<ElementId>) -> Bounds<Pixels> {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.find(id).bounds()).unwrap()
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
    pub(crate) fn next_open_approval(&mut self) -> Seq {
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
        wait_until(what, || {
            self.frame();
            done(self)
        });
    }

    /// Waits until no call the app started is under way, then for the app to take their replies, so whatever a call
    /// started by mistake would have done has been done.
    pub(crate) fn wait_for_every_call(&mut self) {
        self.frame();
        let finished = self.view().client().unwrap().commands().finished();
        self.wait("every call to end", |_| finished.try_recv().is_ok());
        self.frame();
    }

    /// Lets the app catch up. In a test, GPUI draws a window as soon as something in it has changed, so what's checked
    /// next is what the app drew for that change. Nothing here redraws the whole window, as the kit's `render_frame`
    /// would, since that would also redraw a view that never said it changed, and hide that it forgot to `notify`.
    pub(crate) fn frame(&mut self) {
        self.cx.run_until_parked();
    }

    /// Makes the window the one in front, as going back to the app does.
    pub(crate) fn bring_to_front(&mut self) {
        self.cx.update_window(self.handle, |_, window, _| window.activate_window()).unwrap();
        self.frame();
    }

    /// Makes another app the one in front.
    pub(crate) fn send_to_back(&mut self) {
        gpui_kit::VisualTestContext::from_window(self.handle, self.cx).deactivate_window();
        self.frame();
    }

    /// Lets the time pass after which the app looks again whether an older hub has gone idle, and lets it look.
    pub(crate) fn wait_out_a_look(&mut self) {
        self.cx.executor().advance_clock(desktop::app::LOOK_AGAIN);
        self.frame();
    }

    /// Moves the pointer over it, and leaves it there.
    pub(crate) fn hover(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.hover(id, cx)).unwrap();
        self.frame();
    }

    /// Whether the last frame drew it greyed out, so pressing it does nothing.
    pub(crate) fn disabled(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.frame();
        let found = self.cx.update_window(self.handle, |_, window, _| window.try_find(id).and_then(|e| e.disabled()));
        found.unwrap() == Some(true)
    }

    /// Whether the last frame drew it where it can be seen and pressed.
    pub(crate) fn visible(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.try_find(id).is_some_and(|e| e.visible())).unwrap()
    }

    /// Turns the mouse wheel over the element, by `delta`, with the pointer moved to its middle first. Down the page is
    /// a negative `y`.
    ///
    /// Not the kit's `scroll`, which redraws the whole window with `render_frame` before the pointer moves, after it
    /// moves and after the wheel turns. That would hide a view that forgot to `notify`, as [`Ui::frame`] says, and in a
    /// test that turns the wheel many times, those redraws took most of its time.
    pub(crate) fn scroll(&mut self, id: impl Into<ElementId>, delta: ScrollDelta) {
        let id = id.into();
        self.frame();
        let position = self
            .cx
            .update_window(self.handle, |_, window, _| {
                let target = window.find(id.clone());
                assert!(target.visible(), "{id:?} isn't visible to scroll");
                target.bounds().center()
            })
            .unwrap();
        self.cx
            .update_window(self.handle, |_, window, cx| {
                let event = MouseMoveEvent { position, modifiers: window.modifiers(), ..Default::default() };
                window.dispatch_event(event.to_platform_input(), cx);
            })
            .unwrap();
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, cx| {
                let event = ScrollEvent { position, delta, modifiers: window.modifiers(), ..Default::default() };
                window.dispatch_event(event.to_platform_input(), cx);
            })
            .unwrap();
        self.frame();
    }

    pub(crate) fn right_click(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.right_click(id, cx)).unwrap();
        self.frame();
    }

    pub(crate) fn has_dialog(&mut self) -> bool {
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.has_active_dialog(cx)).unwrap()
    }

    pub(crate) fn input(&mut self, text: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
    }

    pub(crate) fn press(&mut self, key: &str) {
        self.press_at_once(&[key]);
    }

    /// Presses each of `keys` before anything they started can answer.
    pub(crate) fn press_at_once(&mut self, keys: &[&str]) {
        self.cx.update_window(self.handle, |_, window, cx| keys.iter().for_each(|key| window.press(key, cx))).unwrap();
        self.frame();
    }

    pub(crate) fn pane(&self) -> Option<Entity<WorkspacePane>> {
        let view = self.view();
        view.selected.as_ref().and_then(|id| view.panes.get(id).cloned())
    }

    pub(crate) fn transcript(&self) -> Option<Entity<Transcript>> {
        self.pane().and_then(|pane| self.cx.read_entity(&pane, |p, _| p.transcript()))
    }

    pub(crate) fn composer_text(&self) -> String {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, cx| p.composer.read(cx).text().to_string())
    }

    /// Opens a picker and clicks the item at this position, the way someone would.
    pub(crate) fn pick(&mut self, picker: &str, position: usize) {
        self.click(SharedString::from(picker.to_string()));
        self.click(ElementId::Integer(position as u64));
    }

    pub(crate) fn chat_mode(&self) -> Option<PermissionMode> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.permission_mode))
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
pub(crate) struct AppViewRef<'a>(pub(super) *const AppView, pub(super) std::marker::PhantomData<&'a ()>);

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
