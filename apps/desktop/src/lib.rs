//! The desktop app. It only talks to a hub, through `client-core`, the same way the CLI does.

mod alert;
pub mod app;
pub mod diff_panel;
mod highlight;
mod menus;
mod sidebar;
pub mod updates;
pub mod workspace_pane;

use gpui_kit::assets::Assets;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions,
    actions, point, px, size,
};

actions!(
    roundhouse,
    [
        About,
        CheckForUpdates,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        Minimize,
        Zoom,
        NewWorkspace,
        AddRepository,
        ToggleDiff,
        CreatePullRequest,
        StopAgent,
        NextWorkspace,
        PreviousWorkspace,
        NewChat,
        CloseChat,
        NextChat,
        PreviousChat
    ]
);

/// Opens the main window on an app that's already running, and returns its view.
pub fn open_main_window(hub: app::HubSource, cx: &mut App) -> anyhow::Result<(AnyWindowHandle, Entity<app::AppView>)> {
    cx.bind_keys([
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-n", NewWorkspace, None),
        KeyBinding::new("cmd-shift-d", ToggleDiff, None),
        KeyBinding::new("cmd-shift-p", CreatePullRequest, None),
        KeyBinding::new("cmd-.", StopAgent, None),
        KeyBinding::new("cmd-]", NextWorkspace, None),
        KeyBinding::new("cmd-[", PreviousWorkspace, None),
        KeyBinding::new("cmd-t", NewChat, None),
        KeyBinding::new("cmd-w", CloseChat, None),
        KeyBinding::new("ctrl-tab", NextChat, None),
        KeyBinding::new("ctrl-shift-tab", PreviousChat, None),
    ]);
    menus::set(cx);
    std::thread::spawn(highlight::preload);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1320.), px(860.)), cx))),
        // No title bar of its own: the sidebar and the workspace header run to the top edge, and the window's
        // buttons sit in the sidebar's empty corner, level with the header. The title still names the
        // window in the Window menu and Mission Control.
        titlebar: Some(TitlebarOptions {
            title: Some("Roundhouse".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(16.), px(16.))),
        }),
        // The app moves the window itself, from the strip along the top. GPUI says AppKit's own handling
        // there holds back each click while it waits for a double click, from macOS 27 on.
        app_owns_titlebar_drag: true,
        focus: true,
        show: true,
        ..Default::default()
    };
    gpui_kit::open_window(options, cx, |window, cx| {
        look::apply(cx);
        window.activate_window();
        cx.new(|cx| app::AppView::new(hub, window, cx))
    })
}

pub fn run() {
    gpui_kit::application().with_assets(Assets).run(|cx| {
        gpui_kit::init(cx);
        cx.activate(true);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let opened = app::HubSource::default_local().and_then(|hub| {
            let data_dir = hub.paths.data_dir.clone();
            open_main_window(hub, cx).map(|_| data_dir)
        });
        match opened {
            Ok(data_dir) => updates::start(&data_dir, cx),
            Err(e) => {
                eprintln!("couldn't open a window: {e:#}");
                cx.quit();
            }
        }
    });
}
