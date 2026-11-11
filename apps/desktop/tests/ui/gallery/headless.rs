//! A view of its own, rather than the whole app, drawn in a window that's never on screen, at the app's scale. The
//! app icon, the pull request's states and the mark's are drawn this way.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, HeadlessAppContext, Pixels, PlatformTextSystem, Render, Size, WindowBounds,
    WindowOptions, point, px,
};

use super::capture::Picture;

/// With the app's theme and assets.
pub(crate) fn app(text_system: &Arc<dyn PlatformTextSystem>) -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        text_system.clone(),
        Arc::new(desktop::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    cx.update(look::apply);
    cx
}

/// The view, in a window exactly as big as `size`. Drawn twice, since one that changes from one frame to the next,
/// such as by moving, can't match from run to run.
pub(crate) fn draw<V: Render>(
    cx: &mut HeadlessAppContext,
    size: Size<Pixels>,
    view: impl FnOnce(&mut Context<V>) -> V + 'static,
) -> Picture {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size))),
        focus: false,
        show: false,
        ..Default::default()
    };
    let handle = cx.update(|cx| cx.open_window(options, |_, cx| cx.new(view))).unwrap();
    let mut frame = || {
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx)).unwrap();
        cx.capture_screenshot(handle.into()).unwrap()
    };
    let (first, image) = (frame(), frame());
    cx.update_window(handle.into(), |_, window, _| window.remove_window()).unwrap();
    cx.run_until_parked();
    Picture { settled: first == image, image }
}
