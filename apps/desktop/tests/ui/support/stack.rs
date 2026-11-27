#![cfg(test)]

use super::daemons;
use client_core::Connection;
use daemons::{binaries, hub_environment};
use desktop::app::{AppView, BuildLine, HubSource, Linking};
use desktop::checkout::Checkout;
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, InputEvent as _, MouseMoveEvent, Pixels, ScrollDelta,
    ScrollEvent, SharedString, TestAppContext,
};
use local_hub::{Daemon, StartedHubs};
use process::testing::WholeEnvironment;
pub(crate) use process::testing::git;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Effort;
use protocol::model::{ModelName, Seq};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use transcript::Transcript;

/// What the hub's Claude gives a chat that names neither, as its adapter describes it.
pub(crate) const CLAUDE_STARTS_WITH: (ModelName, Effort) = (ModelName::named("opus"), Effort::Xhigh);

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Stack {
    pub(crate) dir: TempDir,
    pub(crate) hub: HubSource,
    /// Every hub the stack's daemon starts, whoever asks it to, which the stack stops as it goes.
    pub(super) started: StartedHubs,
}

impl Stack {
    pub(crate) fn new() -> Self {
        Self::in_environment(&hub_environment())
    }

    /// With every hub the app starts in `environment`, which [`hub_environment`] gives with what a test adds.
    pub(crate) fn in_environment(environment: &WholeEnvironment) -> Self {
        let dir = process::testing::tmp_dir("ahui-");
        let root = dir.path();
        process::testing::repository_with_a_commit(&root.join("repo"));
        let paths = protocol::paths::resolve(|name| match name {
            protocol::paths::ENV_HOME => Some(root.join("home").to_string_lossy().into_owned()),
            protocol::paths::ENV_WORKSPACES => Some(root.join("workspaces").to_string_lossy().into_owned()),
            other => std::env::var(other).ok(),
        })
        .unwrap();
        // So the first hub the app starts opens a store that has run every migration, rather than running them all on
        // nothing, which no test here is about. Its data directory is made as the app makes it to start one.
        hub::store::give_a_migrated_database(&paths);
        let started = StartedHubs::default();
        let daemon = Daemon::in_place(binaries().join("daemon_for_tests")).in_environment(environment.vars());
        let hub = HubSource {
            remote: None,
            paths,
            daemon: daemon.recording(&started),
            build_number: None,
            // The daemon's, which was compiled with the same stamp as this test.
            build_version: protocol::this_build!().version(),
        };
        Self { dir, hub, started }
    }

    pub(crate) fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    /// A second client, for setting things up and checking them from outside the app.
    pub(crate) fn client(&self) -> Connection {
        local_hub::connect_or_start(&self.hub.paths, &self.hub.daemon, "test").unwrap()
    }

    /// Where the app keeps its settings.
    pub(crate) fn settings_file(&self) -> PathBuf {
        self.hub.paths.data_dir.join("settings.json")
    }
}

/// Waits for something outside the app, such as a hub before the app has opened. [`Ui::wait`] is for what the app
/// has to draw first.
pub(crate) fn wait_until(what: &str, done: impl FnMut() -> bool) {
    waiting::thread::until(TIMEOUT, format!("timed out waiting for {what}"), done);
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
        ui.wait_to_connect();
        ui
    }

    /// Told it's a build from no checkout, as a stamped one is: what one says of its checkout is tested on the fake hub.
    pub(crate) fn open_without_waiting(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        Self::open_window(cx, stack.hub.clone(), None)
    }

    /// Opens the app's main window on `hub`, told it was built from `checkout`.
    pub(crate) fn open_window(cx: &'a mut TestAppContext, hub: HubSource, checkout: Option<Checkout>) -> Self {
        // A hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.executor().allow_parking();
        // Shutting down waits on those threads, such as for a prompt's call to the hub, for as long by the system's
        // clock as GPUI gives the app. Left as it is, the scheduler gives up after however many of its steps it picks
        // at random, which can be one.
        cx.executor().set_block_on_ticks(usize::MAX..=usize::MAX);
        cx.update(gpui_kit::init);
        // A test's clock stands still, so anything that moves would stay where it starts: a sliding sidebar, a mark,
        // the kit's own animations. `motion.rs` has how a test of something that moves plays it.
        cx.update(|cx| cx.set_reduce_motion(true));
        let (handle, view) =
            cx.update(|cx| desktop::open_main_window(hub, checkout, desktop::Opens::WhereItWasLeft, cx)).unwrap();
        Self { cx, handle, view, seen_approval: Seq::default() }
    }

    pub(super) fn wait_to_connect(&mut self) {
        self.wait("the app to connect", |ui| matches!(ui.view().linking(), Linking::Ready));
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

    /// The newest open approval, once the transcript has it. A session's status comes over the hub's
    /// feed, and the transcript's events over their own subscription, so either can arrive first, and
    /// the transcript can still be showing the previous approval. The transcript can have it before it
    /// draws it, so a test waits for what it clicks, as [`Ui::approve_plan`] does.
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

    /// Runs the app until `done`, on real time, because a hub is on the other end of the socket, answering on threads of
    /// its own. One that times out says what the window showed, such as an error a page drew.
    #[track_caller]
    pub(crate) fn wait(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let frame_then = |ui: &mut Self| {
            ui.frame();
            done(ui)
        };
        waiting::thread::until_or_say(TIMEOUT, what, self, frame_then, Ui::texts_shown);
    }

    /// As [`Ui::wait`], saying what `said` makes of the app as well as what the window shows once it times out, for a
    /// wait on what the window doesn't show, such as which of a chat's rows are loaded.
    #[track_caller]
    pub(crate) fn wait_saying(
        &mut self,
        what: &str,
        mut done: impl FnMut(&mut Self) -> bool,
        said: impl FnOnce(&mut Self) -> String,
    ) {
        let frame_then = |ui: &mut Self| {
            ui.frame();
            done(ui)
        };
        let said = |ui: &mut Self| format!("{}; {}", said(ui), ui.texts_shown());
        waiting::thread::until_or_say(TIMEOUT, what, self, frame_then, said);
    }

    /// Waits until no call the app started is under way, then for the app to take their replies, so whatever a call
    /// started by mistake would have done has been done.
    pub(crate) fn wait_for_every_call(&mut self) {
        self.frame();
        let finished = self.view().hub().commands().finished();
        self.wait("every call to end", |_| finished.try_recv().is_ok());
        self.frame();
    }

    /// Lets the app catch up. In a test, GPUI draws a window as soon as something in it has changed, so what's checked
    /// next is what the app drew for that change. This doesn't redraw the whole window, as the kit's `render_frame`
    /// would, since that would also redraw a view that never said it changed, and hide that it forgot to `notify`.
    /// [`Ui::redraw`] does, only for a test of such a frame.
    pub(crate) fn frame(&mut self) {
        self.cx.run_until_parked();
    }

    /// Draws the whole window once, letting nothing else run, as a frame that something else in it brings does while
    /// what's arrived from the hub, or been parsed on another thread, waits to be taken up. Only for a test of what
    /// that frame shows, since it draws views that didn't say they changed, as [`Ui::frame`] says.
    pub(crate) fn redraw(&mut self) {
        self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx)).unwrap();
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

    /// Whether the last frame drew it, and drew it so it can be pressed. Not of what it didn't draw, so a wait for a
    /// button to come on doesn't end while it's missing.
    pub(crate) fn on(&mut self, id: impl Into<ElementId>) -> bool {
        self.drawn_off(id) == Some(false)
    }

    /// Whether the last frame drew it, and drew it greyed out, so pressing it does nothing. Not of what it didn't draw,
    /// which isn't off but gone.
    pub(crate) fn off(&mut self, id: impl Into<ElementId>) -> bool {
        self.drawn_off(id) == Some(true)
    }

    /// Whether it's off, if the last frame drew it. The kit says only that an element is off, so one it doesn't say
    /// that of is on.
    fn drawn_off(&mut self, id: impl Into<ElementId>) -> Option<bool> {
        let id = id.into();
        self.frame();
        let found = self.cx.update_window(self.handle, |_, window, _| window.try_find(id).map(|e| e.disabled()));
        found.unwrap().map(|off| off == Some(true))
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
        view.selected().and_then(|id| view.panes.get(id).cloned())
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

    pub(crate) fn chat_mode(&self) -> Option<ApprovalMode> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.mode))
    }
}

/// Closes the window. The app it showed lives on until the test has ended, after the stack has stopped its hubs, and
/// connects again meanwhile, which the stack's daemon refuses, as [`Stack`]'s `drop` says.
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
