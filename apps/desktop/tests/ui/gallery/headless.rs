//! A view of its own, rather than the whole app, drawn in a window that's never on screen, at the app's scale. The
//! app icon, the pull request's states and the mark's are drawn this way.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, ElementId, HeadlessAppContext, InputEvent as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, PlatformTextSystem, Render, Size, Window, WindowBounds, WindowOptions, point,
    px,
};
use image::imageops;

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
    draw_at(cx, size, None, view)
}

/// As [`draw`], and at `scale` device pixels to each of the window's where it's given, rather than at the app's. The
/// picture stays as big as it is at the app's scale, with what's drawn at a smaller scale in its top left corner.
pub(crate) fn draw_at<V: Render>(
    cx: &mut HeadlessAppContext,
    size: Size<Pixels>,
    scale: Option<f32>,
    view: impl FnOnce(&mut Context<V>) -> V + 'static,
) -> Picture {
    draw_and_measure(cx, size, scale, view, |_| ()).0
}

/// As [`draw_at`], with what `measure` finds in the window once it's drawn, such as where it drew something.
pub(crate) fn draw_and_measure<V: Render, T>(
    cx: &mut HeadlessAppContext,
    size: Size<Pixels>,
    scale: Option<f32>,
    view: impl FnOnce(&mut Context<V>) -> V + 'static,
    measure: impl FnOnce(&Window) -> T,
) -> (Picture, T) {
    draw_pointing(cx, size, scale, view, &[], measure)
}

/// What the pointer does to a part of a sheet, such as a button: rests over its middle, or presses it there and holds.
#[derive(Clone, Copy)]
pub(crate) enum Pointer {
    Over,
    Pressing,
}

/// As [`draw_and_measure`], with each of `pointed` drawn under the pointer: it's pointed at or pressed for real, one at
/// a time, and its box, by its ID, is taken from that picture into the one with nothing under the pointer. So a sheet
/// shows its parts hovered and pressed side by side, as no one picture of a window could.
pub(crate) fn draw_pointing<V: Render, T>(
    cx: &mut HeadlessAppContext,
    size: Size<Pixels>,
    scale: Option<f32>,
    view: impl FnOnce(&mut Context<V>) -> V + 'static,
    pointed: &[(ElementId, Pointer)],
    measure: impl FnOnce(&Window) -> T,
) -> (Picture, T) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size))),
        focus: false,
        show: false,
        ..Default::default()
    };
    let handle = cx.update(|cx| cx.open_window(options, |_, cx| cx.new(view))).unwrap();
    if let Some(scale) = scale {
        cx.update_window(handle.into(), |_, window, _| window.set_scale_factor(scale)).unwrap();
    }
    let frame = |cx: &mut HeadlessAppContext| {
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx)).unwrap();
        cx.capture_screenshot(handle.into()).unwrap()
    };
    let (first, mut image) = (frame(cx), frame(cx));
    let mut settled = first == image;
    // Nowhere a sheet draws anything that takes the pointer: its top left corner, in its margin.
    let away = point(px(1.), px(1.));
    let send = |cx: &mut HeadlessAppContext, event: gpui_kit::PlatformInput| {
        cx.update_window(handle.into(), |_, window, cx| window.dispatch_event(event, cx)).unwrap();
    };
    for (id, pointer) in pointed {
        let (middle, cut) = cx
            .update_window(handle.into(), |_, window, _| {
                let bounds = window.find(id.clone()).bounds();
                (bounds.center(), in_pixels(bounds, window.scale_factor()))
            })
            .unwrap();
        send(cx, MouseMoveEvent { position: middle, ..Default::default() }.to_platform_input());
        if let Pointer::Pressing = pointer {
            let press =
                MouseDownEvent { button: MouseButton::Left, position: middle, click_count: 1, ..Default::default() };
            send(cx, press.to_platform_input());
        }
        let (once, twice) = (frame(cx), frame(cx));
        settled &= once == twice;
        let (x, y, width, height) = cut;
        imageops::replace(&mut image, &*imageops::crop_imm(&twice, x, y, width, height), x.into(), y.into());
        // Let go away from it, so nothing is clicked, and leave the pointer where it's over nothing.
        let held = MouseMoveEvent { position: away, pressed_button: Some(MouseButton::Left), ..Default::default() };
        if let Pointer::Pressing = pointer {
            send(cx, held.to_platform_input());
            let release =
                MouseUpEvent { button: MouseButton::Left, position: away, click_count: 1, ..Default::default() };
            send(cx, release.to_platform_input());
        }
        send(cx, MouseMoveEvent { position: away, ..Default::default() }.to_platform_input());
    }
    let measured = cx.update_window(handle.into(), |_, window, _| measure(window)).unwrap();
    cx.update_window(handle.into(), |_, window, _| window.remove_window()).unwrap();
    cx.run_until_parked();
    (Picture { image, settled }, measured)
}

/// A box in the window, in a picture's pixels: its left and top edges, and how wide and tall it is, taking in every
/// pixel it reaches into.
fn in_pixels(bounds: Bounds<Pixels>, scale: f32) -> (u32, u32, u32, u32) {
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a box inside a sheet's window")]
    let to = |points: Pixels, rounding: fn(f32) -> f32| rounding(f32::from(points) * scale) as u32;
    let (left, top) = (to(bounds.left(), f32::floor), to(bounds.top(), f32::floor));
    (left, top, to(bounds.right(), f32::ceil) - left, to(bounds.bottom(), f32::ceil) - top)
}
