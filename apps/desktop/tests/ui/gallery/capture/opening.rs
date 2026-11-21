//! Opening the real app on a screen's hub, in a window of its own that's never on screen.

use std::path::Path;
use std::sync::Arc;

use desktop::app::Connection;
use gpui_kit::{HeadlessAppContext, PlatformTextSystem, px, size};

use super::Driver;
use super::scrollbar::ChatScroll;
use client_core::fake_hub;

use crate::gallery::frame;
use crate::support::hub_source;

/// The device the app is on in every screen, by its ID, which a world puts a chat's conversation on to have it here.
pub(crate) const THIS_MAC: &str = "this-mac";

/// The real app, connected to a hub that holds `world`, if there's one, in a directory of its own, which goes with it,
/// and in a [`frame::WIDE`] window if it's `wide`.
pub(super) fn open(
    world: Option<fn() -> fake_hub::World>,
    wide: bool,
    text_system: &Arc<dyn PlatformTextSystem>,
) -> (tempfile::TempDir, Driver) {
    // Under /tmp because of the socket path limit.
    let dir = tempfile::Builder::new().prefix("gallery-").tempdir_in("/tmp").unwrap();
    let hub = hub_source(dir.path());
    if let Some(world) = world {
        std::fs::create_dir_all(&hub.paths.data_dir).unwrap();
        this_mac(&hub.paths.data_dir);
        let built = world();
        if !built.saved_hubs.is_empty() {
            write_saved_hubs(&hub.paths.data_dir, &built.saved_hubs);
        }
        fake_hub::serve(&hub.paths.socket, built);
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
    cx.update(|cx| {
        look::hold_still(fake_hub::NOW_MS, cx);
        desktop::app::the_device_name_is("Ada’s MacBook Air", cx);
    });
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
    app.wait("the app to connect", |view, _| !matches!(view.connection(), Connection::Connecting));
    (dir, app)
}

/// Has the app be on the device [`THIS_MAC`], with its voice's files here, so a chat whose conversation is on it
/// reads nothing from Hugging Face. They're as big as they say they are and empty, which only their size says is there,
/// and nothing reads from them unless something is read out, which no screen does.
fn this_mac(data_dir: &Path) {
    std::fs::write(data_dir.join("device-id"), THIS_MAC).unwrap();
    let voice = conversation::FILES.dir(&data_dir.join("speaking").join("phonon"));
    std::fs::create_dir_all(&voice).unwrap();
    for file in conversation::FILES.files {
        std::fs::File::create(voice.join(file.path)).unwrap().set_len(file.size_bytes).unwrap();
    }
}

fn write_saved_hubs(data_dir: &Path, hubs: &[(&str, &str)]) {
    let entries: Vec<_> = hubs
        .iter()
        .enumerate()
        .map(|(ix, (name, host))| {
            let byte = u8::try_from(ix + 1).unwrap_or(1);
            let fp = protocol::pairing::Fingerprint::of_sha256([byte; 32]);
            serde_json::json!({
                "name": name,
                "hosts": [host],
                "fingerprint": fp.as_str(),
                "device": format!("saved-dev-{}", ix + 1),
            })
        })
        .collect();
    let json = serde_json::json!({ "hubs": entries });
    std::fs::write(data_dir.join("settings.json"), serde_json::to_vec(&json).unwrap()).unwrap();
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
