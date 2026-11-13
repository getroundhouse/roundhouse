//! The desktop app. It only talks to a hub, through `client-core`, the same way the CLI does.

#[cfg(target_os = "macos")]
mod accessibility_focus;
mod ago;
mod alert;
pub mod all_files;
pub mod app;
pub mod appimage;
mod assets;
mod attached;
pub mod busy;
pub mod changes;
pub mod checkout;
mod composing;
pub mod file_diff;
mod file_header;
mod file_icons;
mod file_image;
mod file_tree;
pub mod file_view;
mod haptics;
mod home;
pub mod image_diff;
mod keys;
pub mod lost_release;
mod menus;
mod model_picker;
pub mod new_workspace;
mod notifications;
pub mod patch;
mod prompt_cache;
pub mod pull_request;
mod rename;
mod reorder;
pub mod review;
mod right_click;
mod saving;
mod settings;
mod sidebar;
pub mod sidebar_tabs;
mod sides;
mod sounds;
pub mod updates;
pub mod why_quit;
#[cfg(target_os = "windows")]
mod window_handle;
mod window_place;
mod workspace_order;
pub mod workspace_pane;

// The build script's `.res` writer, which puts the app's icon in the program on Windows. Its tests run here, since a
// build script's never do.
#[cfg(test)]
#[path = "../build/app_icon/res.rs"]
mod app_icon_res;

pub use assets::Assets;
use checkout::Checkout;
use gpui_kit::{AnyWindowHandle, App, AppContext, Entity, TitlebarOptions, WindowOptions, actions, point};
pub use window_place::Opens;

/// How long a list's order holds still after the pointer last moved over it, the sidebar's or Home's, which a test can
/// wait out.
#[cfg(feature = "testing")]
pub const ORDER_HOLDS_FOR: std::time::Duration = reorder::HOLDS_FOR;

/// How long after the window last moved its place is saved, which a test can wait out.
#[cfg(feature = "testing")]
pub const WINDOW_SETTLES_AFTER: std::time::Duration = window_place::SETTLES_AFTER;

/// The sounds the app has played, by name, for a test to read.
#[cfg(feature = "testing")]
pub use sounds::played as sounds_played;

actions!(
    roundhouse,
    [
        About,
        CheckForUpdates,
        Hide,
        HideOthers,
        ShowAll,
        OpenSettings,
        CloseSettings,
        FindSetting,
        Quit,
        Minimize,
        Zoom,
        ToggleMenus,
        NewWorkspace,
        AddRepository,
        ArchiveWorkspace,
        MarkWorkspace,
        ToggleDiff,
        FlipImage,
        CreatePullRequest,
        StopAgent,
        NextWorkspace,
        PreviousWorkspace,
        NewChat,
        CloseChat,
        NextChat,
        PreviousChat,
        TogglePlanMode,
        FindInChat,
        FindNext,
        FindPrevious
    ]
);

/// Opens the main window on an app that's already running, and returns its view. `checkout` is the one this build is
/// from, which the window names, or `None` for a stamped build, or a picture of the app.
pub fn open_main_window(
    hub: app::HubSource,
    checkout: Option<Checkout>,
    opens: Opens,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<app::AppView>)> {
    // Before any notification, which Windows shows only for an app that has said what it's called.
    cx.set_app_identity(notifications::APP_ID, "Roundhouse");
    settings::load(&hub.paths.data_dir, cx);
    #[cfg(target_os = "macos")]
    accessibility_focus::install();
    keys::set(cx);
    menus::set(hub.build_version, cx);
    std::thread::spawn(look::preload_highlighting);
    let (display_id, window_bounds) = window_place::open_at(opens, &hub.paths.data_dir, cx);
    let options = WindowOptions {
        window_bounds: Some(window_bounds),
        display_id,
        // No title bar of its own: the sidebar and the workspace header run to the top edge, and on a Mac the
        // window's buttons sit in the sidebar's empty corner, level with the header. On Windows the app draws
        // them, in `app::window_buttons`. The title still names the window in the Window menu and Mission Control.
        titlebar: Some(TitlebarOptions {
            title: Some(checkout.as_ref().map_or_else(|| "Roundhouse".to_string(), Checkout::window_title).into()),
            appears_transparent: true,
            traffic_light_position: Some(point(sides::WINDOW_BUTTONS_INSET, sides::WINDOW_BUTTONS_INSET)),
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
        cx.new(|cx| {
            window_place::follow(window, cx);
            app::AppView::new(hub, checkout, window, cx)
        })
    })
}

/// Runs the app, which is `build`, from `checkout`, until it quits.
pub fn run(build: protocol::Build, checkout: Option<Checkout>) {
    gpui_kit::application().with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        // Only the app as it runs plays sounds, so a test or a picture of the app is quiet.
        cx.set_global(sounds::Speaker);
        cx.activate(true);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        why_quit::say(checkout.as_ref(), cx);
        let opened = app::HubSource::default_local(build).and_then(|hub| {
            let data_dir = hub.paths.data_dir.clone();
            open_main_window(hub, checkout, Opens::WhereItWasLeft, cx).map(|_| data_dir)
        });
        match opened {
            Ok(data_dir) => updates::start(&data_dir, cx),
            Err(e) => {
                eprintln!("couldn't open a window: {e:#}");
                // Not by quitting, which exits with 0, as an app that someone quit does. Nothing is open to wait for.
                std::process::exit(1);
            }
        }
    });
}
