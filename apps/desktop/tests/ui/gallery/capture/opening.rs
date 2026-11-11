//! Opening the real app on a screen's hub, in a window of its own that's never on screen.

use std::path::Path;
use std::sync::Arc;

use client_core::Daemon;
use desktop::app::{Connection, HubSource};
use gpui_kit::{HeadlessAppContext, PlatformTextSystem, px, size};

use super::Driver;
use super::scrollbar::ChatScroll;
use crate::gallery::{fake_hub, frame, world};

/// The real app, connected to a hub that holds `world`, if there's one, in a directory of its own, which goes with it,
/// and in a [`frame::WIDE`] window if it's `wide`.
pub(super) fn open(
    world: Option<fn() -> world::World>,
    wide: bool,
    text_system: &Arc<dyn PlatformTextSystem>,
) -> (tempfile::TempDir, Driver) {
    // Under /tmp because of the socket path limit.
    let dir = tempfile::Builder::new().prefix("gallery-").tempdir_in("/tmp").unwrap();
    let hub = hub_source(dir.path());
    if let Some(world) = world {
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
    // Nothing moves, so a mark holds still and anything else, such as a dialog's entrance, shows as it ends. The time
    // holds still too, so how long something took or how long ago it was is the same on every run.
    cx.update(|cx| look::hold_still(world::NOW_MS, cx));
    // The app loads the highlighter on a thread of its own, and draws code plain until it has, so load it first for
    // a picture of the app once it's ready.
    look::preload_highlighting();
    // As people see it, so no checkout.
    let (handle, view) =
        cx.update(|cx| desktop::open_main_window(hub, None, desktop::Opens::InItsUsualPlace, cx)).unwrap();
    let mut app = Driver { cx, handle, view, dir: dir.path().to_path_buf(), chat: ChatScroll::default() };
    if wide {
        app.widen();
    }
    app.wait("the app to connect", |view, _| !matches!(view.connection, Connection::Connecting));
    (dir, app)
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

impl Driver {
    /// Makes the window [`frame::WIDE`] wide, and as tall as it was. A headless window has no edge to drag, so it's
    /// resized, and then told so, as the platform's resize callback would tell it.
    fn widen(&mut self) {
        self.cx
            .update_window(self.handle, |_, window, cx| {
                window.resize(size(px(frame::WIDE), window.viewport_size().height));
                window.bounds_changed(cx);
            })
            .unwrap();
    }
}
