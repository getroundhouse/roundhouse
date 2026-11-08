//! The desktop app. It only talks to a hub, through `client-core`, the same way the CLI does.

pub mod app;
pub mod diff_panel;
mod highlight;
mod sidebar;
pub mod workspace_pane;

use gpui_kit::assets::Assets;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions,
    actions, px, size,
};

actions!(
    roundhouse,
    [
        Quit,
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
        KeyBinding::new("cmd-q", Quit, None),
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
    cx.on_action(|_: &Quit, cx| cx.quit());
    std::thread::spawn(highlight::preload);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1320.), px(860.)), cx))),
        titlebar: Some(TitlebarOptions { title: Some("Roundhouse".into()), ..Default::default() }),
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
        if let Err(e) = app::HubSource::default_local().and_then(|hub| open_main_window(hub, cx)) {
            eprintln!("couldn't open a window: {e:#}");
            cx.quit();
        }
    });
}
