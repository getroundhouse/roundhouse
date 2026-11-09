//! Opens the real app on a screen's hub, in a window that's never on screen, and takes its picture.

#![cfg(test)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use client_core::Daemon;
use desktop::app::{AppView, Connection, HubSource};
use desktop::updates::{Kind, Problem};
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, App, Entity, HeadlessAppContext, PlatformTextSystem};
use image::RgbaImage;

use super::fake_hub;
use super::screens::{Screen, Step};
use crate::support::labelled;

/// Long enough for the app to load a screen from a hub on the same machine, many times over.
const TIMEOUT: Duration = Duration::from_secs(20);
/// How many pictures in a row have to match before a screen counts as finished drawing.
const SAME_IN_A_ROW: usize = 3;
const BETWEEN_PICTURES: Duration = Duration::from_millis(50);
/// A tooltip, or the update card's details, shows once the pointer has rested on it for half a second.
const LONGER_THAN_A_TOOLTIP_WAITS: Duration = Duration::from_secs(1);

pub(crate) struct Picture {
    pub(crate) image: RgbaImage,
    /// It stopped changing. If not, something on the screen moves, and the picture won't match from run to run.
    pub(crate) settled: bool,
}

pub(crate) fn take(screen: &Screen, text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    // Under /tmp because of the socket path limit.
    let dir = tempfile::Builder::new().prefix("gallery-").tempdir_in("/tmp").unwrap();
    let hub = hub_source(dir.path());
    if let Some(world) = screen.world {
        std::fs::create_dir_all(&hub.paths.data_dir).unwrap();
        fake_hub::serve(&hub.paths.socket, world());
    }
    let mut cx = HeadlessAppContext::with_platform(
        text_system.clone(),
        Arc::new(desktop::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    // The hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
    cx.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(look::hold_still);
    // The kit's own animations, such as a dialog's entrance, show as they end.
    cx.update(|cx| cx.set_reduce_motion(true));
    // As people see it, so no checkout.
    let (handle, view) = cx.update(|cx| desktop::open_main_window(hub, None, cx)).unwrap();
    let mut app = Driver { cx, handle, view };
    app.wait("the app to connect", |view, _| !matches!(view.connection, Connection::Connecting));
    for step in screen.steps {
        app.step(step);
    }
    let picture = app.settle();
    app.close();
    picture
}

/// Somewhere of its own, with a daemon that doesn't exist, so the app can't start a real hub in its place. The
/// daemon's path is the same every run, because the app shows it when it can't start one.
fn hub_source(dir: &Path) -> HubSource {
    let paths = protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(dir.join("home").to_string_lossy().into_owned()),
        protocol::paths::ENV_WORKSPACES => Some(dir.join("workspaces").to_string_lossy().into_owned()),
        _ => None,
    })
    .unwrap();
    let daemon = Daemon::in_place("/nonexistent/roundhoused");
    HubSource { paths, daemon, build_number: Some(fake_hub::BUILD_NUMBER), build_version: fake_hub::BUILD }
}

struct Driver {
    cx: HeadlessAppContext,
    handle: AnyWindowHandle,
    view: Entity<AppView>,
}

impl Driver {
    fn step(&mut self, step: &Step) {
        match step {
            Step::Select(name) => {
                let view = self.view.clone();
                self.cx.update(|cx| {
                    view.update(cx, |v, cx| {
                        let id = v.workspaces.iter().find(|w| &w.name == name).map(|w| w.id.clone()).unwrap();
                        v.select(id, cx);
                    });
                });
                self.wait("the chat to load", |view, cx| {
                    selected_pane(view).is_some_and(|pane| {
                        let pane = pane.read(cx);
                        let wanted = pane.session().map_or(0, |s| s.last_seq);
                        pane.transcript().is_some_and(|t| t.read(cx).loaded_events() as u64 >= wanted)
                    })
                });
            }
            Step::OpenDiff => self.open_diff(),
            Step::OpenFile(path) => self.open_file(path),
            Step::StartComment(line) => self.start_comment(*line),
            Step::Click(id) => self.click(*id),
            Step::HoverTab(position) => self.hover_tab(*position),
            Step::Type(text) => {
                self.frame();
                self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
            }
            Step::Hover { on, shows } => self.hover(on, shows),
            Step::AskToFinish => {
                self.step(&Step::Click("finish-update"));
                self.wait("the question", |view, _| view.update_question.is_some());
            }
            Step::RightClick(id) => self.right_click(id),
            Step::ClickItem(label) => self.click_item(label),
            Step::UpdateReady(version) => self.cx.update(|cx| desktop::updates::show_ready(version, cx)),
            Step::UpdateProblem => {
                let problem = Problem { kind: Kind::Unreachable, detail: "connection refused".to_string() };
                self.cx.update(|cx| desktop::updates::show_problem(problem, cx));
            }
            Step::OpenSettings => {
                let open = Box::new(desktop::OpenSettings);
                self.cx.update_window(self.handle, |_, window, cx| window.dispatch_action(open, cx)).unwrap();
            }
            Step::OpenSetupLog => {
                self.update_pane(WorkspacePane::toggle_setup_log);
                self.wait("the setup log to load", |view, cx| {
                    selected_pane(view)
                        .is_some_and(|pane| pane.read(cx).setup_log.as_ref().is_some_and(|t| !t.is_empty()))
                });
            }
        }
    }

    fn hover_tab(&mut self, position: usize) {
        self.frame();
        let view = self.view.clone();
        let chat = self.cx.update(|cx| {
            let pane = selected_pane(view.read(cx)).unwrap();
            let open: Vec<_> = pane.read(cx).sessions.iter().filter(|s| s.closed_at_ms.is_none()).collect();
            open[position].id.clone()
        });
        let id = gpui_kit::SharedString::from(format!("chat-{chat}"));
        self.cx.update_window(self.handle, |_, window, cx| window.hover(id, cx)).unwrap();
    }

    fn right_click(&mut self, id: &'static str) {
        self.frame();
        let id = gpui_kit::SharedString::from(id);
        self.cx.update_window(self.handle, |_, window, cx| window.right_click(id, cx)).unwrap();
        // The menu opens on the next frame.
        self.frame();
    }

    fn click_item(&mut self, label: &str) {
        self.frame();
        let id = self.cx.update_window(self.handle, |_, window, _| labelled(window, label)).unwrap();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
    }

    fn click(&mut self, id: impl Into<gpui_kit::SharedString>) {
        self.frame();
        let id = id.into();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
    }

    fn open_diff(&mut self) {
        self.update_pane(WorkspacePane::toggle_changes);
        self.wait("the diff to load", |view, cx| {
            let pane = selected_pane(view);
            pane.is_some_and(|pane| pane.read(cx).diff.as_ref().is_some_and(|d| d.review.read(cx).files.is_some()))
        });
    }

    fn open_file(&mut self, path: &str) {
        self.click(format!("file-{path}"));
        self.wait("the diff tab to show it", |view, cx| {
            let file = selected_pane(view).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.file.clone()));
            file.is_some_and(|file| file.read(cx).file(cx).is_some_and(|file| file.path() == path))
        });
    }

    fn start_comment(&mut self, line: usize) {
        let view = self.view.clone();
        let file = self.cx.update(|cx| {
            selected_pane(view.read(cx)).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.file.clone()))
        });
        self.cx
            .update_window(self.handle, |_, window, cx| {
                file.unwrap().update(cx, |f, cx| {
                    let path = f.path.clone().unwrap();
                    f.start_comment(DiffLinePosition::new(path, DiffSide::Modified, line), window, cx);
                });
            })
            .unwrap();
        self.frame();
    }

    fn update_pane(&mut self, change: impl FnOnce(&mut WorkspacePane, &mut gpui_kit::Context<WorkspacePane>)) {
        let view = self.view.clone();
        self.cx.update(|cx| {
            let pane = selected_pane(view.read(cx)).unwrap();
            pane.update(cx, change);
        });
    }

    /// Draws until [`SAME_IN_A_ROW`] pictures match, or the time's up.
    fn settle(&mut self) -> Picture {
        let deadline = Instant::now() + TIMEOUT;
        let (mut last, mut same) = (self.picture(), 1);
        while same < SAME_IN_A_ROW && Instant::now() < deadline {
            std::thread::sleep(BETWEEN_PICTURES);
            let next = self.picture();
            same = if next == last { same + 1 } else { 1 };
            last = next;
        }
        Picture { image: last, settled: same >= SAME_IN_A_ROW }
    }

    fn picture(&mut self) -> RgbaImage {
        self.frame();
        self.cx.capture_screenshot(self.handle).unwrap()
    }

    fn wait(&mut self, what: &str, mut done: impl FnMut(&AppView, &App) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            let view = self.view.clone();
            if self.cx.update(|cx| done(view.read(cx), cx)) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Rests the pointer on `on`, and waits until the window draws `shows`, such as its tooltip.
    fn hover(&mut self, on: &'static str, shows: &'static str) {
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.hover(on, cx)).unwrap();
        // The app's clock only moves when told to, and a tooltip waits for it.
        self.cx.run_until_parked();
        self.cx.advance_clock(LONGER_THAN_A_TOOLTIP_WAITS);
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            if self.cx.update_window(self.handle, |_, window, _| window.try_find(shows).is_some()).unwrap() {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {shows}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn frame(&mut self) {
        self.cx.run_until_parked();
        self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        self.cx.run_until_parked();
    }

    /// Before the hub goes, so the app doesn't see it go and try to reconnect.
    fn close(mut self) {
        self.cx.update_window(self.handle, |_, window, _| window.remove_window()).unwrap();
        self.cx.run_until_parked();
    }
}

fn selected_pane(view: &AppView) -> Option<Entity<WorkspacePane>> {
    view.selected.as_ref().and_then(|id| view.panes.get(id).cloned())
}
