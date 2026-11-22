//! Draws a design at the size of a phone's screen, in a window that's never on screen, and keeps its picture in
//! `.context/designs/phone/`. A design is drawn from the design system's parts, with made-up words and numbers, and a
//! chat in one is the real transcript, on a fake hub.

#![cfg(test)]

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use client_core::fake_hub::{self, World};
use client_core::{Client, RecordsFeed};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, AssetSource, Bounds, Context, Entity, HeadlessAppContext, IntoElement,
    PlatformTextSystem, Render, SharedString, Window, WindowBounds, WindowOptions, point, px, size,
};
use transcript::Transcript;

/// An iPhone's screen, in points.
const SCREEN: (f32, f32) = (402., 874.);
const TIMEOUT: Duration = Duration::from_secs(20);

// The icons the designs draw that neither the app nor what every client shares embeds yet.
gpui_kit::assets::icon_assets!(
    Extra,
    [
        Archive,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        AudioLines,
        Bell,
        BellRing,
        Bot,
        Camera,
        ChevronsUpDown,
        CircleCheck,
        CircleStop,
        CircleX,
        Copy,
        Clock,
        EllipsisVertical,
        ExternalLink,
        Eye,
        FileDiff,
        Folder,
        FolderOpen,
        Github,
        GitBranch,
        Globe,
        Image,
        Info,
        Keyboard,
        KeyRound,
        Laptop,
        ListTodo,
        MessageSquare,
        MessageSquarePlus,
        MessagesSquare,
        Mic,
        Paperclip,
        Pencil,
        RefreshCw,
        ScanLine,
        Search,
        Server,
        Settings,
        SlidersHorizontal,
        Smartphone,
        Sparkles,
        Square,
        SquareDot,
        SquarePlus,
        SquareTerminal,
        Trash,
        TriangleAlert,
        User,
        Volume2,
        WifiOff,
        X,
    ]
);

#[derive(Clone, Copy)]
struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        match Extra.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => mobile::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut found = Extra.list(path)?;
        found.extend(mobile::Assets.list(path)?);
        Ok(found)
    }
}

/// What a design draws, each time its window does.
type Draws = Box<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Design(Draws);

impl Render for Design {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.0)(window, cx)
    }
}

/// A chat a design shows: the real transcript, and what keeps its hub there.
pub(super) struct Chat {
    pub(super) transcript: Entity<Transcript>,
    _dir: tempfile::TempDir,
}

pub(super) struct Canvas {
    cx: HeadlessAppContext,
    out: PathBuf,
}

impl Canvas {
    pub(super) fn new(text_system: &Arc<dyn PlatformTextSystem>) -> Canvas {
        look::Reach::Handheld.choose();
        let mut cx = HeadlessAppContext::with_platform(
            text_system.clone(),
            Arc::new(Icons),
            gpui_kit::platform::current_headless_renderer,
        );
        // A chat's hub answers on real threads.
        cx.allow_parking();
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            look::hold_still(fake_hub::NOW_MS, cx);
            look::apply(cx);
            look::Input::Touch.choose(cx);
        });
        look::preload_highlighting();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/designs/phone");
        std::fs::create_dir_all(&out).unwrap();
        Canvas { cx, out }
    }

    /// The chat last worked in of the workspace called `workspace`, in a hub that holds `world`.
    pub(super) fn chat(&mut self, world: World, workspace: &str) -> Chat {
        let dir = tempfile::Builder::new().prefix("phone-designs-").tempdir_in("/tmp").unwrap();
        let socket = dir.path().join("hub.sock");
        fake_hub::serve(&socket, world);
        let client = Client::connect(&socket, "designs").unwrap();
        let feed = RecordsFeed::start(&client);
        waiting::thread::until(TIMEOUT, "timed out waiting for the hub's records", || {
            self.cx.run_until_parked();
            feed.connected()
        });
        let records = feed.snapshot();
        let id = records.workspaces().iter().find(|w| w.name == workspace).unwrap().id.clone();
        let session = records.sessions(&id).iter().rfind(|s| s.is_open()).unwrap().clone();
        feed.follow_session_records(&session.id);
        let agent = records.agents().of(session.agent);
        let transcript = self.cx.update(|cx| {
            cx.new(|cx| {
                let mut transcript = Transcript::new(client, &session, cx);
                transcript.set_agent(agent, cx);
                transcript.follow_records(feed.clone());
                transcript.records_changed(&records, cx);
                transcript
            })
        });
        Chat { transcript, _dir: dir }
    }

    /// Draws the design `draws` makes, once `ready` says what it shows has loaded, and keeps its picture as `name`.
    pub(super) fn draw_once(
        &mut self,
        name: &str,
        ready: impl Fn(&App) -> bool,
        draws: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) {
        let screen = Bounds::new(point(px(0.), px(0.)), size(px(SCREEN.0), px(SCREEN.1)));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(screen)),
            focus: false,
            show: false,
            ..Default::default()
        };
        let draws: Draws = Box::new(draws);
        let handle = self.cx.update(|cx| cx.open_window(options, |_, cx| cx.new(|_| Design(draws)))).unwrap();
        let frame = |cx: &mut HeadlessAppContext| {
            cx.run_until_parked();
            cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx)).unwrap();
            cx.run_until_parked();
            cx.capture_screenshot(handle.into()).unwrap()
        };
        let deadline = Instant::now() + TIMEOUT;
        while !self.cx.update(|cx| ready(cx)) {
            assert!(Instant::now() < deadline, "timed out waiting for {name} to load");
            frame(&mut self.cx);
            waiting::thread::for_what_mustnt_happen(Duration::from_millis(20));
        }
        let (mut last, mut same) = (frame(&mut self.cx), 1);
        while same < 3 && Instant::now() < deadline {
            waiting::thread::for_what_mustnt_happen(Duration::from_millis(30));
            let next = frame(&mut self.cx);
            same = if next == last { same + 1 } else { 1 };
            last = next;
        }
        assert!(same >= 3, "{name} keeps changing");
        last.save(self.out.join(format!("{name}.png"))).unwrap();
        self.cx.update_window(handle.into(), |_, window, _| window.remove_window()).unwrap();
        self.cx.run_until_parked();
    }

    /// Draws a design that shows nothing that loads.
    pub(super) fn draw(&mut self, name: &str, draws: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) {
        self.draw_once(name, |_| true, draws);
    }
}

/// Whether a chat's transcript has every row it holds.
pub(super) fn loaded(chat: &Chat, cx: &App) -> bool {
    let transcript = chat.transcript.read(cx);
    transcript.loaded_events() == transcript.row_count() && transcript.row_count() > 0
}
