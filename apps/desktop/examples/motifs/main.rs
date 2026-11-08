//! SKETCH, never merged: three choices of motif for the status mark, live, in a window of their own.
//!
//!     cargo run -p desktop --example motifs

mod framework;
#[allow(dead_code)]
mod view;

use gpui_kit::assets::Assets;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    gpui_kit::application().with_assets(Assets).run(|cx| {
        gpui_kit::init(cx);
        cx.activate(true);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1560.), px(920.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("Roundhouse motifs".into()), ..Default::default() }),
            focus: true,
            show: true,
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            Theme::change(ThemeMode::Dark, Some(window), cx);
            desktop::apply_theme(Some(window), cx);
            window.activate_window();
            cx.new(|_| framework::Framework::new())
        })
        .expect("open a window");
    });
}
