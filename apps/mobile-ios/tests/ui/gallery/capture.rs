//! Opens the real app on a screen's hub, in a window the size of a phone's screen that's never on screen, does what
//! the screen says, and takes its picture.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use client_core::{Client, ClientError, RemoteHub, fake_hub, stand_in_hub};
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, Bounds, Edges, ElementId, Entity, GestureTuning, HeadlessAppContext, InputEvent as _, Pixels,
    PlatformTextSystem, Point, SharedString, TouchEvent, TouchId, TouchPhase, WindowBounds, WindowOptions, point, px,
    size,
};
use image::RgbaImage;
use mobile::{MemoryVault, MobileApp, Vault as _};
use protocol::client::{ApiError, CLIENT_PROTOCOL_VERSION, HubRefusing};
use protocol::pairing::Fingerprint;
use protocol::versions::Versions;
use spec_screens::Picture;

use super::screens::{Hub, Screen, Step};

/// Long enough for the app to load a screen from a hub on the same machine, many times over.
const TIMEOUT: Duration = Duration::from_secs(20);
/// How many pictures in a row have to match before a screen counts as finished drawing.
const SAME_IN_A_ROW: usize = 3;
const BETWEEN_PICTURES: Duration = Duration::from_millis(50);

/// An iPhone's screen, in points, and what of it the status bar and the home indicator are drawn over.
const SCREEN: (f32, f32) = (402., 874.);
const STATUS_BAR: f32 = 62.;
const HOME_INDICATOR: f32 = 34.;

/// What the made-up hub is called, and where a phone would look for it.
pub(super) const HUB_NAME: &str = "Ada’s Mac mini";
pub(super) const HUB_HOSTS: [&str; 2] = ["adas-mac-mini.local:7421", "192.168.1.20:7421"];

/// The screen's picture, of the whole window.
pub(super) fn take(screen: &Screen, text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut phone = Phone::open(&screen.hub, text_system);
    for step in screen.steps {
        phone.step(step);
    }
    let picture = phone.settle();
    phone.close();
    picture
}

struct Phone {
    cx: HeadlessAppContext,
    handle: AnyWindowHandle,
    app: Entity<MobileApp>,
    /// Pairing links, as the Camera app opens them.
    links: flume::Sender<String>,
    /// Whether the hub can still be reached, for a screen that loses it.
    reachable: Arc<AtomicBool>,
    /// The screen's own directory, for its hub's socket, which goes with it.
    _dir: tempfile::TempDir,
}

impl Phone {
    fn open(hub: &Hub, text_system: &Arc<dyn PlatformTextSystem>) -> Phone {
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("phone-gallery-").tempdir_in("/tmp").unwrap();
        let socket = dir.path().join("hub.sock");
        let vault = Arc::new(MemoryVault::default());
        let reachable = Arc::new(AtomicBool::new(true));
        let mut fails: fn(&RemoteHub) -> ClientError = unreachable;
        match hub {
            Hub::Unpaired => {}
            Hub::Holds(world) => {
                fake_hub::serve(&socket, world());
                vault.save(&paired()).unwrap();
            }
            Hub::NeverAnswers => {
                stand_in_hub::listen(&socket, |_| stand_in_hub::Does::Nothing);
                vault.save(&paired()).unwrap();
            }
            Hub::Older => {
                reachable.store(false, Ordering::Relaxed);
                fails = older;
                vault.save(&paired()).unwrap();
            }
            Hub::Unreachable => {
                reachable.store(false, Ordering::Relaxed);
                vault.save(&paired()).unwrap();
            }
        }
        let mut cx = HeadlessAppContext::with_platform(
            text_system.clone(),
            Arc::new(mobile::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        // The hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.allow_parking();
        cx.update(gpui_kit::init);
        // Nothing moves, so a mark holds still, and the time holds still too, so how long ago something was is the
        // same on every run.
        cx.update(|cx| {
            look::hold_still(fake_hub::NOW_MS, cx);
            let safe_area = Edges { top: px(STATUS_BAR), bottom: px(HOME_INDICATOR), ..Edges::default() };
            mobile::the_safe_area_is(safe_area, cx);
        });
        // The app loads the highlighter on a thread of its own, and draws code plain until it has.
        look::preload_highlighting();
        let (links, opened) = flume::unbounded();
        let screen = Bounds::new(point(px(0.), px(0.)), size(px(SCREEN.0), px(SCREEN.1)));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(screen)),
            focus: false,
            show: false,
            ..Default::default()
        };
        let connect = connecting_to(socket, reachable.clone(), fails);
        let (handle, app) =
            cx.update(|cx| mobile::open_window_connecting(vault, opened, connect, options, cx)).unwrap();
        let mut phone = Phone { cx, handle, app, links, reachable, _dir: dir };
        if let Hub::Holds(_) = hub {
            phone.until("the app to connect", |phone| phone.cx.update(|cx| phone.app.read(cx).is_connected(cx)));
        }
        phone
    }

    fn step(&mut self, step: &Step) {
        match step {
            Step::Tap(id) => self.tap(id),
            Step::OpenChat(row) => {
                self.tap(row);
                self.until("the chat to load", |phone| {
                    phone.cx.update(|cx| loaded(phone.app.read(cx).transcript(cx), cx))
                });
            }
            Step::OpenSubagent(open) => {
                self.tap(open);
                self.until("the subagent's work to load", |phone| {
                    phone.cx.update(|cx| loaded(phone.app.read(cx).subagent_transcript(cx), cx))
                });
            }
            Step::Type(text) => {
                self.frame();
                self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
            }
            Step::OpenLink(link) => {
                self.links.send(link()).unwrap();
            }
            Step::WaitFor(id) => self.wait_for(&id_of(id)),
            Step::PressLong(id) => self.press_long(id_of(id)),
            Step::PressLongNumbered(name) => {
                let first = move |window: &gpui_kit::Window| {
                    let ids = window.elements().into_iter().filter_map(|e| e.path().last().cloned());
                    ids.into_iter().find(|id| matches!(id, ElementId::NamedInteger(named, _) if named == name))
                };
                self.until(&format!("something whose ID is {name} and a number"), |phone| {
                    phone.cx.update_window(phone.handle, |_, window, _| first(window).is_some()).unwrap()
                });
                let found = self.cx.update_window(self.handle, |_, window, _| first(window).unwrap()).unwrap();
                self.press_long(found);
            }
            Step::TapStarting(part) => self.tap_starting(part),
            Step::Choose(label) => self.choose(label),
            Step::LoseTheHub => {
                self.reachable.store(false, Ordering::Relaxed);
                let app = self.app.clone();
                self.cx.update(|cx| app.update(cx, MobileApp::connect_again));
                self.until("the app to find the hub gone", |phone| {
                    phone.cx.update(|cx| !phone.app.read(cx).is_connected(cx))
                });
            }
        }
    }

    /// Holds a finger on `id` until its menu opens, then lets go.
    fn press_long(&mut self, id: ElementId) {
        self.wait_for(&id);
        let at = self.cx.update_window(self.handle, |_, window, _| window.find(id).bounds().center()).unwrap();
        self.touch(TouchPhase::Started, at);
        // The app's clock is the test's, which moves only when it's told to.
        self.cx.advance_clock(GestureTuning::default().long_press_duration + Duration::from_millis(1));
        self.wait_for(&id_of(MENU));
        self.touch(TouchPhase::Ended, at);
    }

    /// Taps the first element drawn whose ID starts with `part`, once there is one.
    fn tap_starting(&mut self, part: &'static str) {
        let first = |window: &gpui_kit::Window| {
            let ids = window.elements().into_iter().filter_map(|e| e.path().last().cloned());
            ids.into_iter().find(|id| matches!(id, ElementId::Name(name) if name.starts_with(part)))
        };
        self.until(&format!("something whose ID starts {part}"), |phone| {
            phone.cx.update_window(phone.handle, |_, window, _| first(window).is_some()).unwrap()
        });
        let found = self.cx.update_window(self.handle, |_, window, _| first(window).unwrap()).unwrap();
        self.cx.update_window(self.handle, |_, window, cx| window.click(found, cx)).unwrap();
    }

    /// Taps the row labelled `label` in the menu that's open.
    fn choose(&mut self, label: &'static str) {
        let menu = id_of(MENU);
        self.wait_for(&menu);
        let (place, row) = self
            .cx
            .update_window(self.handle, |_, window, _| {
                let mut found = window.find_all_by_label(label).into_iter();
                let path =
                    found.find(|found| found.path().contains(&menu)).expect("the row is in the menu").path().to_vec();
                // By its place in the menu too, since two rows can say the same, such as two untitled chats.
                (path[path.len() - 2].clone(), path[path.len() - 1].clone())
            })
            .unwrap();
        let scope = menu.clone();
        self.cx.update_window(self.handle, |_, window, cx| window.within(scope).within(place).click(row, cx)).unwrap();
    }

    fn touch(&mut self, phase: TouchPhase, at: Point<Pixels>) {
        let touch = TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None };
        self.cx
            .update_window(self.handle, |_, window, cx| window.dispatch_event(touch.to_platform_input(), cx))
            .unwrap();
    }

    /// Something drawn from what's still loading can't be tapped until it's there.
    fn tap(&mut self, id: &'static str) {
        let id = id_of(id);
        self.wait_for(&id);
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
    }

    fn wait_for(&mut self, id: &ElementId) {
        self.until(&format!("{id:?} to be drawn"), |phone| {
            let found = id.clone();
            phone.cx.update_window(phone.handle, |_, window, _| window.try_find(found).is_some()).unwrap()
        });
    }

    /// Draws until `done`, and fails once the time's up.
    fn until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        waiting::thread::until(TIMEOUT, format!("timed out waiting for {what}"), || {
            self.frame();
            done(self)
        });
    }

    /// Draws until [`SAME_IN_A_ROW`] pictures match, or the time's up.
    fn settle(&mut self) -> Picture {
        let deadline = Instant::now() + TIMEOUT;
        let (mut last, mut same) = (self.picture(), 1);
        while same < SAME_IN_A_ROW && Instant::now() < deadline {
            // What mustn't happen is the screen changing between pictures.
            waiting::thread::for_what_mustnt_happen(BETWEEN_PICTURES);
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

    fn frame(&mut self) {
        self.cx.run_until_parked();
        self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        self.cx.run_until_parked();
    }

    /// Before the hub goes, so the app doesn't see it go and try to connect again.
    fn close(mut self) {
        self.cx.update_window(self.handle, |_, window, _| window.remove_window()).unwrap();
        self.cx.run_until_parked();
    }
}

/// The ID the kit gives the menu that's open.
const MENU: &str = "popup-menu";

fn id_of(id: &'static str) -> ElementId {
    SharedString::from(id).into()
}

/// Whether there's a transcript with every row it holds.
fn loaded(transcript: Option<gpui_kit::Entity<transcript::Transcript>>, cx: &gpui_kit::App) -> bool {
    transcript.is_some_and(|transcript| {
        let transcript = transcript.read(cx);
        transcript.loaded_events() == transcript.row_count() && transcript.row_count() > 0
    })
}

/// What the phone keeps of the hub it paired with.
fn paired() -> RemoteHub {
    RemoteHub {
        hub_name: HUB_NAME.to_string(),
        hosts: HUB_HOSTS.map(str::to_string).to_vec(),
        fingerprint: Fingerprint::of_sha256([1; 32]),
        device_id: "dev-1".into(),
        token: "made-up".into(),
    }
}

/// Connects to the screen's hub on its socket, while it's `reachable`, and fails as `fails` says otherwise, such as a
/// phone off the hub's network does.
fn connecting_to(
    socket: PathBuf,
    reachable: Arc<AtomicBool>,
    fails: fn(&RemoteHub) -> ClientError,
) -> impl Fn(&RemoteHub) -> Result<Client, ClientError> + Send + Sync + 'static {
    move |hub| {
        if reachable.load(Ordering::Relaxed) { connect(&socket) } else { Err(fails(hub)) }
    }
}

fn connect(socket: &Path) -> Result<Client, ClientError> {
    Client::connect(socket, "roundhouse-ios")
}

fn unreachable(hub: &RemoteHub) -> ClientError {
    let timed_out = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    ClientError::Unreachable { hosts: hub.hosts.clone(), last: Some(timed_out), not_pinned: Vec::new() }
}

/// The refusal of a hub that's a protocol break behind the app.
fn older(_: &RemoteHub) -> ClientError {
    let speaks = Versions::only(CLIENT_PROTOCOL_VERSION - 1);
    ApiError::versions_differ("refused", HubRefusing { version: "0.9.0".to_string(), speaks }).into()
}
